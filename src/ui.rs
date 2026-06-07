//! Builders for the "About This Computer" dialog body.
//!
//! [`build_about_box`] turns a [`SystemInfo`] into the saudade [`Container`]
//! that the binary embeds in its window — and that `examples/screenshot.rs`
//! drives through `MockBackend` to refresh the README image. The Snake-game
//! easter egg lives in `main.rs`; everything here is pure layout, so a
//! screenshot can render the dialog without dragging in tick-driven state.

use saudade::{Container, Font, Label, Painter, Rect, SvgImage, Theme, Widget, include_svg};

/// Default window width. Grown at runtime when the headline machine name
/// would otherwise overflow the value column.
pub const MIN_CONTENT_WIDTH: i32 = 320;
/// Pixel size used for the headline machine name in the Overview.
pub const MACHINE_FONT_SIZE: f32 = 20.0;

/// Position and size of the OS logo in the Overview section, in the root
/// widget's logical coordinate system. The overall window height is derived
/// from the stacked sections at build time rather than fixed.
pub const LOGO_X: i32 = 16;
pub const LOGO_Y: i32 = 12;
pub const LOGO_W: i32 = 56;
pub const LOGO_H: i32 = 56;

/// Bounding box of the OS logo. The binary uses this for its easter-egg hit
/// test; it's exposed here so both stay in sync if the logo ever moves.
pub const LOGO_HIT: Rect = Rect::new(LOGO_X, LOGO_Y, LOGO_W, LOGO_H);

/// Horizontal layout columns (logical px). The logo, key labels and the rules
/// all left-align to `KEY_X`; the detail values and the Overview header text
/// all left-align to `VALUE_X`, so the big machine name sits over the value
/// column of the sections below it. The right edge of the rule (and so the
/// value column) is `content_width - RULE_X`, derived at runtime since the
/// window may grow to fit a long headline.
const RULE_X: i32 = 16;
const KEY_X: i32 = RULE_X;
const VALUE_X: i32 = 90;
/// Height of one key/value detail row.
const ROW_H: i32 = 18;
/// Gap between a section's last line and the rule beneath it. Applied to both
/// rules so the Overview text clears the first rule by the same amount the
/// Disk row clears the second.
const RULE_GAP: i32 = 8;

/// Everything the about box needs to render — gathered live by the binary from
/// `sysinfo` + `host`, or hand-rolled by the screenshot example for stable
/// output.
pub struct SystemInfo {
    /// Headline shown large in the Overview group: the friendly model name
    /// (e.g. "MacBook Pro"), or the short hostname when the OS exposes no
    /// product info (e.g. NetBSD).
    pub machine: String,
    /// pfetch-style full OS name (e.g. "macOS 26.5"). Shown under the machine
    /// name as the Overview's second line.
    pub operating_system: String,
    /// Hardware vendor as reported by the firmware (e.g. "Apple Inc."). Shown
    /// in the Hardware group; `None` hides the row where unavailable.
    pub vendor: Option<String>,
    /// Raw machine model identifier (e.g. "MacBookPro18,3", "20AN"). Shown in
    /// the Hardware group; `None` hides the row where the OS has no such id.
    pub model: Option<String>,
    pub cpu: String,
    pub memory_line: String,
    pub disk_line: String,
    /// Kernel name and version (e.g. "Darwin 25.5.0"). Shown in the Software
    /// group.
    pub kernel: String,
    /// Compositor (Wayland) or window manager (X11) plus the windowing system,
    /// e.g. "River (Wayland)". Shown in the Software group; `None` hides the row
    /// on a tty session or a platform without the concept.
    pub window_manager: Option<String>,
    /// Number of installed packages, when the OS exposes a cheap way to count
    /// them. `None` hides the row.
    pub packages: Option<u32>,
    /// Human-readable uptime, formatted like pfetch ("1d 3h 20m").
    pub uptime: String,
    /// Identifier the OS-release file (or, on the BSDs / macOS / Windows,
    /// `std::env::consts::OS`) reports — picks which baked logo is drawn.
    /// Unknown ids fall back to the generic Tux mark.
    pub distribution_id: String,
}

/// Pick a window width that fits every value-column string at its rendering
/// size: the firmware-reported product family at the large headline size, and
/// every detail row's value at the body font size. Falls back to
/// [`MIN_CONTENT_WIDTH`] when the system font can't be loaded for measurement.
///
/// `family` is the firmware-reported product family used for headline sizing
/// (separate from `info.machine`, which may be the prettified name or a
/// hostname fallback). Pass [`None`] to skip the headline pass.
pub fn compute_content_width(info: &SystemInfo, family: Option<&str>, body_size: f32) -> i32 {
    let Some(font) = Font::load_system() else {
        return MIN_CONTENT_WIDTH;
    };
    content_width_with_font(info, family, body_size, &font)
}

/// Same as [`compute_content_width`] but using a caller-supplied font — lets
/// the screenshot example measure with the bundled DejaVu face so the window
/// size is deterministic across machines.
pub fn content_width_with_font(
    info: &SystemInfo,
    family: Option<&str>,
    body_size: f32,
    font: &Font,
) -> i32 {
    let needed_for = |text: &str, size: f32| -> i32 {
        let (w, _) = font.measure(text, size);
        VALUE_X + w.ceil() as i32 + RULE_X
    };

    let mut required = MIN_CONTENT_WIDTH;

    if let Some(family) = family.map(str::trim).filter(|s| !s.is_empty()) {
        required = required.max(needed_for(family, MACHINE_FONT_SIZE));
    }

    let mut values: Vec<&str> = vec![
        info.operating_system.as_str(),
        info.cpu.as_str(),
        info.memory_line.as_str(),
        info.disk_line.as_str(),
        info.kernel.as_str(),
        info.uptime.as_str(),
    ];
    if let Some(v) = &info.vendor {
        values.push(v);
    }
    if let Some(m) = &info.model {
        values.push(m);
    }
    if let Some(wm) = &info.window_manager {
        values.push(wm);
    }
    let pkg_str;
    if let Some(n) = info.packages {
        pkg_str = n.to_string();
        values.push(&pkg_str);
    }
    for v in values {
        required = required.max(needed_for(v, body_size));
    }

    required
}

/// Build the about-box container at `content_width` logical pixels wide. The
/// container sizes its own height from the stacked groups; the caller is
/// expected to query `bounds().h` and match the window to it. The "Close"
/// button is wired to `cx.close()`.
pub fn build_about_box(info: &SystemInfo, content_width: i32) -> Container {
    let rule_w = content_width - 2 * RULE_X;
    let content_right = RULE_X + rule_w;

    // Hardware rows: vendor and model only when the OS reports them.
    let mut hardware: Vec<(&str, &str)> = Vec::new();
    if let Some(vendor) = &info.vendor {
        hardware.push(("Vendor", vendor));
    }
    if let Some(model) = &info.model {
        hardware.push(("Model", model));
    }
    hardware.push(("CPU", &info.cpu));
    hardware.push(("Memory", &info.memory_line));
    hardware.push(("Disk", &info.disk_line));

    let mut software: Vec<(&str, &str)> = Vec::new();
    software.push(("System", &info.kernel));
    if let Some(wm) = &info.window_manager {
        software.push(("WM", wm));
    }
    let packages_str;
    if let Some(n) = info.packages {
        packages_str = n.to_string();
        software.push(("Packages", &packages_str));
    }
    software.push(("Uptime", &info.uptime));

    // Overview header text: the machine name (large) over the OS line, aligned
    // to the value column and stopping short of the OK button at the top-right.
    // The block sits low — its bottom clears the first rule by `RULE_GAP`, just
    // like the last detail row clears the second — which lets the logo ride
    // higher with room above the rule.
    let ok_w = 72;
    let ok_h = 24;
    let machine_top = 24;
    let machine_box = Rect::new(VALUE_X, machine_top, rule_w, 26);
    let os_box = Rect::new(VALUE_X, machine_top + 24, rule_w, ROW_H);

    // Vertical rhythm: two rules carving Overview | Hardware | Software.
    let rule1_y = os_box.bottom() + RULE_GAP;

    let hardware_top = rule1_y + 13;
    let rule2_y = hardware_top + hardware.len() as i32 * ROW_H + RULE_GAP;
    let software_top = rule2_y + 13;
    let software_bottom = software_top + software.len() as i32 * ROW_H;

    // OK button: bottom-right of the Overview, its bottom clearing the first
    // rule by the same RULE_GAP the header text's bottom does.
    let ok = Rect::new(
        content_right / 2 - ok_w / 2,
        software_bottom + RULE_GAP * 2,
        ok_w,
        ok_h,
    );

    let mut root = Container::new(content_width, ok.bottom() + 12);

    // --- Overview: logo, header text, OK button ---
    root.push(build_os_logo(
        LOGO_X,
        LOGO_Y,
        LOGO_W,
        LOGO_H,
        &info.distribution_id,
    ));
    root.push(Label::new(machine_box, info.machine.clone()).with_size(MACHINE_FONT_SIZE));
    root.push(Label::new(os_box, info.operating_system.clone()));

    // --- Dividers + the two detail sections ---
    push_rows(&mut root, hardware_top, &hardware, content_right);
    push_rows(&mut root, software_top, &software, content_right);

    root.push(
        saudade::Button::new(ok, "Close")
            .default(true)
            .on_click(|cx| cx.close()),
    );

    root
}

/// Lay key/value rows from `top_y` down: keys left-aligned at `KEY_X`, values
/// left-aligned at `VALUE_X` so they form a clean column.
fn push_rows(root: &mut Container, top_y: i32, rows: &[(&str, &str)], content_right: i32) {
    let value_w = content_right - VALUE_X;
    let mut y = top_y;
    for (key, value) in rows {
        root.push(Label::new(
            Rect::new(KEY_X, y, VALUE_X - KEY_X - 6, ROW_H),
            format!("{key}:"),
        ));
        root.push(Label::new(
            Rect::new(VALUE_X, y, value_w, ROW_H),
            (*value).to_string(),
        ));
        y += ROW_H;
    }
}

/// Build the OS-logo widget for a given distribution id and hand it a
/// compile-time-baked SVG via [`logo_svg_for`].
pub fn build_os_logo(x: i32, y: i32, w: i32, h: i32, distribution_id: &str) -> Logo {
    Logo {
        rect: Rect::new(x, y, w, h),
        image: logo_svg_for(distribution_id),
    }
}

/// An OS logo drawn from a compile-time-baked [`SvgImage`]. `include_svg!`
/// flattens each mark into solid-color polygons at build time, so the binary
/// ships no SVG parser; [`Painter::draw_svg`] fills those polygons aspect-fit
/// and centered in `rect`, at the live device resolution — the mark stays sharp
/// at any DPI or size, with anti-aliased edges blended over the real background.
pub struct Logo {
    rect: Rect,
    image: SvgImage,
}

impl Widget for Logo {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn paint(&mut self, painter: &mut Painter, _theme: &Theme) {
        painter.draw_svg(&self.image, self.rect);
    }
}

/// Pick the logo SVG for an os-release `ID` (as returned by
/// [`sysinfo::System::distribution_id`], which falls back to
/// [`std::env::consts::OS`] when there is no os-release file — that is how the
/// BSDs and macOS/Windows are matched). Known major distros, the BSDs, macOS
/// and Windows get their own mark; everything else — including unrecognised
/// Linux distros and the bare `"linux"` fallback — falls back to the generic
/// Tux penguin.
pub fn logo_svg_for(distribution_id: &str) -> SvgImage {
    // Baked at compile time by `include_svg!`, whose path is resolved relative
    // to the crate's `CARGO_MANIFEST_DIR` (not this source file), so the names
    // start from the crate root.
    const APPLE: SvgImage = include_svg!("assets/os/apple.svg");
    const ARCH: SvgImage = include_svg!("assets/os/arch.svg");
    const DEBIAN: SvgImage = include_svg!("assets/os/debian.svg");
    const FEDORA: SvgImage = include_svg!("assets/os/fedora.svg");
    const FREEBSD: SvgImage = include_svg!("assets/os/freebsd.svg");
    const MINT: SvgImage = include_svg!("assets/os/linuxmint.svg");
    const MANJARO: SvgImage = include_svg!("assets/os/manjaro.svg");
    const NETBSD: SvgImage = include_svg!("assets/os/netbsd.svg");
    const NIXOS: SvgImage = include_svg!("assets/os/nixos.svg");
    const OPENBSD: SvgImage = include_svg!("assets/os/openbsd.svg");
    const OPENSUSE: SvgImage = include_svg!("assets/os/opensuse.svg");
    // tux.svg uses SVG features (a filter, group opacity) that `include_svg!`
    // can't bake; the macro flags this via a `deprecated` warning we can't act on.
    #[allow(deprecated)]
    const TUX: SvgImage = include_svg!("assets/os/tux.svg");
    const UBUNTU: SvgImage = include_svg!("assets/os/ubuntu.svg");
    const WINDOWS: SvgImage = include_svg!("assets/os/windows.svg");

    match distribution_id {
        "ubuntu" => UBUNTU,
        "debian" => DEBIAN,
        "fedora" => FEDORA,
        "arch" => ARCH,
        "manjaro" => MANJARO,
        "linuxmint" => MINT,
        "nixos" => NIXOS,
        "macos" => APPLE,
        "windows" => WINDOWS,
        "freebsd" => FREEBSD,
        "openbsd" => OPENBSD,
        "netbsd" => NETBSD,
        // openSUSE ships several IDs: "opensuse", "opensuse-leap",
        // "opensuse-tumbleweed", ...
        id if id.starts_with("opensuse") => OPENSUSE,
        _ => TUX,
    }
}
