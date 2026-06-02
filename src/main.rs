use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use resvg::{tiny_skia, usvg};
use retrofetch::host;
use saudade::EventCtx;
use saudade::{
    App, Button, Color, Container, Event, Image, Key, Label, MouseButton, NamedKey, Painter,
    PopupRequest, Rect, Theme, Widget, WindowConfig,
};
use sysinfo::{Disks, System};

const CONTENT_WIDTH: i32 = 320;

/// Position and size of the OS logo in the Overview section, in the root
/// widget's logical coordinate system. The overall window height is derived
/// from the stacked sections at build time rather than fixed.
const LOGO_X: i32 = 16;
const LOGO_Y: i32 = 12;
const LOGO_W: i32 = 56;
const LOGO_H: i32 = 56;

/// Bounding box of the OS logo. Five clicks here within `EASTER_EGG_WINDOW`
/// swap the about chrome for a Snake game.
const LOGO_HIT: Rect = Rect::new(LOGO_X, LOGO_Y, LOGO_W, LOGO_H);
const EASTER_EGG_CLICKS: usize = 5;
const EASTER_EGG_WINDOW: Duration = Duration::from_secs(2);

struct SystemInfo {
    /// Headline shown large in the Overview group: the friendly model name
    /// (e.g. "MacBook Pro"), or the short hostname when the OS exposes no
    /// product info (e.g. NetBSD).
    machine: String,
    /// pfetch-style full OS name (e.g. "macOS 26.5"). Shown under the machine
    /// name as the Overview's second line.
    operating_system: String,
    /// Hardware vendor as reported by the firmware (e.g. "Apple Inc."). Shown
    /// in the Hardware group; `None` hides the row where unavailable.
    vendor: Option<String>,
    /// Raw machine model identifier (e.g. "MacBookPro18,3", "20AN"). Shown in
    /// the Hardware group; `None` hides the row where the OS has no such id.
    model: Option<String>,
    cpu: String,
    memory_line: String,
    disk_line: String,
    /// Kernel name and version (e.g. "Darwin 25.5.0"). Shown in the Software
    /// group.
    kernel: String,
    /// Human-readable uptime, formatted like pfetch ("1d 3h 20m").
    uptime: String,
}

fn main() {
    let info = gather_system_info();

    let about = build_about_box(&info);
    // The box sizes its own height from the stacked group boxes; match the
    // window to it so there's no dead strip at the bottom.
    let height = about.bounds().h;
    let root = AboutWithEasterEgg::new(about);

    App::new(
        WindowConfig::new("About This Computer", CONTENT_WIDTH, height),
        root,
    )
    .with_theme(Theme::windows_31())
    .run();
}

/// Horizontal layout columns (logical px). The logo, key labels and the rules
/// all left-align to `KEY_X`; the detail values and the Overview header text
/// all left-align to `VALUE_X`, so the big machine name sits over the value
/// column of the sections below it.
const RULE_X: i32 = 16;
const RULE_W: i32 = CONTENT_WIDTH - 2 * RULE_X;
const CONTENT_RIGHT: i32 = RULE_X + RULE_W;
const KEY_X: i32 = RULE_X;
const VALUE_X: i32 = 90;
/// Height of one key/value detail row.
const ROW_H: i32 = 18;
/// Gap between a section's last line and the rule beneath it. Applied to both
/// rules so the Overview text clears the first rule by the same amount the
/// Disk row clears the second.
const RULE_GAP: i32 = 8;

fn build_about_box(info: &SystemInfo) -> Container {
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

    let software: [(&str, &str); 2] = [("System", &info.kernel), ("Uptime", &info.uptime)];

    // Overview header text: the machine name (large) over the OS line, aligned
    // to the value column and stopping short of the OK button at the top-right.
    // The block sits low — its bottom clears the first rule by `RULE_GAP`, just
    // like the last detail row clears the second — which lets the logo ride
    // higher with room above the rule.
    let ok_w = 72;
    let ok_h = 24;
    let machine_top = 24;
    let machine_box = Rect::new(VALUE_X, machine_top, RULE_W, 26);
    let os_box = Rect::new(VALUE_X, machine_top + 24, RULE_W, ROW_H);

    // Vertical rhythm: two rules carving Overview | Hardware | Software.
    let rule1_y = os_box.bottom() + RULE_GAP;

    let hardware_top = rule1_y + 13;
    let rule2_y = hardware_top + hardware.len() as i32 * ROW_H + RULE_GAP;
    let software_top = rule2_y + 13;
    let software_bottom = software_top + software.len() as i32 * ROW_H;

    // OK button: bottom-right of the Overview, its bottom clearing the first
    // rule by the same RULE_GAP the header text's bottom does.
    let ok = Rect::new(
        CONTENT_RIGHT / 2 - ok_w / 2,
        software_bottom + RULE_GAP * 2,
        ok_w,
        ok_h,
    );

    let mut root = Container::new(CONTENT_WIDTH, ok.bottom() + 12);

    // --- Overview: logo, header text, OK button ---
    root.push(build_os_logo(LOGO_X, LOGO_Y, LOGO_W, LOGO_H));
    root.push(Label::new(machine_box, info.machine.clone()).with_size(20.0));
    root.push(Label::new(os_box, info.operating_system.clone()));

    // --- Dividers + the two detail sections ---
    push_rows(&mut root, hardware_top, &hardware);
    push_rows(&mut root, software_top, &software);

    root.push(
        Button::new(ok, "Close")
            .default(true)
            .on_click(|cx| cx.close()),
    );

    root
}

/// Lay key/value rows from `top_y` down: keys left-aligned at `KEY_X`, values
/// left-aligned at `VALUE_X` so they form a clean column.
fn push_rows(root: &mut Container, top_y: i32, rows: &[(&str, &str)]) {
    let value_w = CONTENT_RIGHT - VALUE_X;
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

/// Build the OS-logo image: detect the host OS/distro, rasterize its embedded
/// SVG into the logo box, and hand the pixels to an [`Image`] widget. If the
/// SVG can't be parsed/rendered the logo is simply left blank rather than
/// failing the whole dialog.
fn build_os_logo(x: i32, y: i32, w: i32, h: i32) -> Image {
    let svg = logo_svg_for(&System::distribution_id());
    match rasterize_logo(svg, w, h) {
        Some(pixels) => Image::from_pixels(x, y, w, h, pixels),
        None => Image::new(x, y, w, h),
    }
}

/// Pick the logo SVG for an os-release `ID` (as returned by
/// [`System::distribution_id`], which falls back to [`std::env::consts::OS`]
/// when there is no os-release file — that is how the BSDs and macOS/Windows
/// are matched). Known major distros, the BSDs, macOS and Windows get their
/// own mark; everything else — including unrecognised Linux distros and the
/// bare `"linux"` fallback — falls back to the generic Tux penguin.
fn logo_svg_for(distribution_id: &str) -> &'static str {
    const APPLE: &str = include_str!("../assets/os/apple.svg");
    const ARCH: &str = include_str!("../assets/os/arch.svg");
    const DEBIAN: &str = include_str!("../assets/os/debian.svg");
    const FEDORA: &str = include_str!("../assets/os/fedora.svg");
    const FREEBSD: &str = include_str!("../assets/os/freebsd.svg");
    const MINT: &str = include_str!("../assets/os/linuxmint.svg");
    const MANJARO: &str = include_str!("../assets/os/manjaro.svg");
    const NETBSD: &str = include_str!("../assets/os/netbsd.svg");
    const NIXOS: &str = include_str!("../assets/os/nixos.svg");
    const OPENBSD: &str = include_str!("../assets/os/openbsd.svg");
    const OPENSUSE: &str = include_str!("../assets/os/opensuse.svg");
    const TUX: &str = include_str!("../assets/os/tux.svg");
    const UBUNTU: &str = include_str!("../assets/os/ubuntu.svg");
    const WINDOWS: &str = include_str!("../assets/os/windows.svg");

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

/// Rasterize an SVG into a `w`×`h` ARGB32 buffer for [`Image::from_pixels`].
///
/// `tiny_skia` renders premultiplied RGBA. The `Image` widget skips fully
/// transparent pixels and otherwise writes the colour opaquely (it does not
/// blend), so anti-aliased edges are composited over white — the about-box
/// background — here, and fully transparent pixels are left transparent so
/// that background still shows through.
fn rasterize_logo(svg: &str, w: i32, h: i32) -> Option<Vec<u32>> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let (w, h) = (w as u32, h as u32);

    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;

    // Scale the SVG to fit the box while preserving aspect ratio, then centre.
    let size = tree.size();
    let scale = (w as f32 / size.width()).min(h as f32 / size.height());
    let tx = (w as f32 - size.width() * scale) * 0.5;
    let ty = (h as f32 - size.height() * scale) * 0.5;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut out = vec![0u32; (w * h) as usize];
    for (px, chunk) in out.iter_mut().zip(pixmap.data().chunks_exact(4)) {
        let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        if a == 0 {
            continue;
        }
        // Premultiplied source over an opaque white background:
        // out = src + white * (1 - a). With white == 255, that is src + (255 - a).
        let inv = 255 - a as u32;
        let comp = |c: u8| (c as u32 + inv).min(255);
        *px = 0xFF00_0000 | (comp(r) << 16) | (comp(g) << 8) | comp(b);
    }
    Some(out)
}

/// Root widget. Normally a transparent wrapper around the about-box
/// container — but five clicks on the system logo within two seconds
/// flip the window into Snake mode: the chrome disappears, the surface
/// turns white, and the existing window becomes the game's playfield.
struct AboutWithEasterEgg {
    about: Container,
    snake: Option<SnakeGame>,
    /// Timestamps of recent left-button presses inside `LOGO_HIT`.
    /// Trimmed to the trailing 2-second window on every new click.
    logo_clicks: Vec<Instant>,
}

impl AboutWithEasterEgg {
    fn new(about: Container) -> Self {
        Self {
            about,
            snake: None,
            logo_clicks: Vec::new(),
        }
    }

    /// Record a fresh click on the logo and return `true` when the
    /// 5-clicks-in-2-seconds threshold has been crossed.
    fn register_logo_click(&mut self, now: Instant) -> bool {
        self.logo_clicks
            .retain(|t| now.duration_since(*t) <= EASTER_EGG_WINDOW);
        self.logo_clicks.push(now);
        self.logo_clicks.len() >= EASTER_EGG_CLICKS
    }
}

impl Widget for AboutWithEasterEgg {
    fn bounds(&self) -> Rect {
        self.about.bounds()
    }

    fn paint(&mut self, painter: &mut Painter, theme: &Theme) {
        match &mut self.snake {
            Some(game) => game.paint(painter, self.about.bounds()),
            None => self.about.paint(painter, theme),
        }
    }

    fn event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(game) = self.snake.as_mut() {
            // Escape leaves the easter egg and restores the about box.
            // Handled here, not inside the game, because only the wrapper
            // owns the about/snake mode switch.
            if let Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } = event
            {
                self.snake = None;
                ctx.request_paint();
                return;
            }
            game.event(event, ctx);
            return;
        }

        // Detect the easter-egg trigger before the about box gets the
        // event — buttons inside the container don't overlap the logo
        // so swallowing the click here is invisible to them.
        if let Event::PointerDown {
            pos,
            button: MouseButton::Left,
        } = event
            && LOGO_HIT.contains(*pos)
            && self.register_logo_click(Instant::now())
        {
            self.snake = Some(SnakeGame::new(self.about.bounds()));
            self.logo_clicks.clear();
            ctx.request_paint();
            return;
        }

        self.about.event(event, ctx);
    }

    fn layout(&mut self, bounds: Rect) {
        self.about.layout(bounds);
    }

    fn popup_request(&self) -> Option<PopupRequest> {
        if self.snake.is_some() {
            None
        } else {
            self.about.popup_request()
        }
    }

    fn wants_ticks(&self) -> bool {
        self.snake.is_some()
    }
}

// -------------------------------------------------------------------- Snake

const CELL: i32 = 10;
const STEP_INTERVAL: Duration = Duration::from_millis(110);
/// Reserved strip at the top of the surface for the score "title bar".
const HUD_TOP: i32 = 20;
/// Reserved strip at the bottom of the surface for the key hints.
const HUD_BOTTOM: i32 = 18;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn delta(self) -> (i32, i32) {
        match self {
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }

    fn opposite(self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

/// Classic Snake. The grid is sized to the about-box content area at
/// construction time and the game uses the existing window as its
/// playfield — no separate window, no chrome.
struct SnakeGame {
    grid_w: i32,
    grid_h: i32,
    /// Cell offset inside `bounds` so the grid is centered when the
    /// playfield doesn't divide evenly into 10px cells.
    offset_x: i32,
    offset_y: i32,
    body: VecDeque<(i32, i32)>,
    /// Direction the snake is currently moving.
    direction: Direction,
    /// Direction inputs queued for upcoming steps. Each `step` consumes
    /// one entry. Buffering matters when the player taps a corner
    /// faster than the step interval: without a queue, the second
    /// press would clobber the first and one of the turns would be
    /// silently dropped.
    pending: VecDeque<Direction>,
    food: (i32, i32),
    rng_state: u64,
    last_step: Instant,
    game_over: bool,
    score: u32,
}

/// Carve the playfield out of the full window surface, leaving the
/// HUD strips at top and bottom untouched.
fn play_rect(bounds: Rect) -> Rect {
    Rect::new(
        bounds.x,
        bounds.y + HUD_TOP,
        bounds.w,
        (bounds.h - HUD_TOP - HUD_BOTTOM).max(CELL),
    )
}

impl SnakeGame {
    fn new(bounds: Rect) -> Self {
        let play = play_rect(bounds);
        let grid_w = (play.w / CELL).max(8);
        let grid_h = (play.h / CELL).max(8);
        // Center the grid inside the playfield so any leftover pixels
        // sit as an even margin instead of a one-sided gap.
        let offset_x = (play.w - grid_w * CELL) / 2;
        let offset_y = (play.h - grid_h * CELL) / 2;

        let mut body = VecDeque::new();
        let cx = grid_w / 2;
        let cy = grid_h / 2;
        body.push_back((cx, cy));
        body.push_back((cx - 1, cy));
        body.push_back((cx - 2, cy));

        // Seed the RNG from wall-clock nanoseconds so the food placement
        // isn't identical every time the easter egg is triggered.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let seed = nanos ^ (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);

        let mut g = Self {
            grid_w,
            grid_h,
            offset_x,
            offset_y,
            body,
            direction: Direction::Right,
            pending: VecDeque::new(),
            food: (0, 0),
            rng_state: seed.max(1),
            last_step: Instant::now(),
            game_over: false,
            score: 0,
        };
        g.place_food();
        g
    }

    /// Maximum number of inputs we buffer ahead of the current step.
    /// Two is enough for a snappy corner ("up, left" from a rightward
    /// run) without letting the snake feel like it's running a script
    /// instead of responding to the player.
    const MAX_PENDING: usize = 2;

    /// Add a direction to the input queue, rejecting no-ops (same as
    /// the last queued/current direction) and 180° reversals — those
    /// would cause the snake to instantly collide with itself.
    fn queue_direction(&mut self, dir: Direction) {
        let last = self.pending.back().copied().unwrap_or(self.direction);
        if dir == last || dir == last.opposite() {
            return;
        }
        if self.pending.len() >= Self::MAX_PENDING {
            return;
        }
        self.pending.push_back(dir);
    }

    fn rand_u32(&mut self) -> u32 {
        // Standard 64-bit xorshift — good enough for picking a cell.
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        (x >> 32) as u32
    }

    fn place_food(&mut self) {
        loop {
            let x = (self.rand_u32() % self.grid_w as u32) as i32;
            let y = (self.rand_u32() % self.grid_h as u32) as i32;
            if !self.body.iter().any(|c| *c == (x, y)) {
                self.food = (x, y);
                return;
            }
        }
    }

    fn step(&mut self) {
        // Consume one buffered input per step. `queue_direction`
        // already filters out no-ops and reversals, so anything that
        // reaches us here is a valid turn.
        if let Some(next) = self.pending.pop_front() {
            self.direction = next;
        }
        let &(hx, hy) = self.body.front().expect("snake body is non-empty");
        let (dx, dy) = self.direction.delta();
        let head = (hx + dx, hy + dy);

        let hit_wall = head.0 < 0 || head.0 >= self.grid_w || head.1 < 0 || head.1 >= self.grid_h;
        // Tail tip moves out of the way every step, so it isn't a
        // collision unless we're growing this frame.
        let will_grow = head == self.food;
        let hit_self = self
            .body
            .iter()
            .enumerate()
            .any(|(i, c)| *c == head && (will_grow || i + 1 < self.body.len()));

        if hit_wall || hit_self {
            self.game_over = true;
            return;
        }

        self.body.push_front(head);
        if will_grow {
            self.score += 1;
            self.place_food();
        } else {
            self.body.pop_back();
        }
    }

    fn reset(&mut self) {
        // Rebuild the playing state in-place. We can't go through
        // `SnakeGame::new` because that would re-derive the grid from
        // a synthetic `bounds`, and `play_rect` would inset the HUD
        // strips a second time — shrinking the grid on every restart.
        self.body.clear();
        let cx = self.grid_w / 2;
        let cy = self.grid_h / 2;
        self.body.push_back((cx, cy));
        self.body.push_back((cx - 1, cy));
        self.body.push_back((cx - 2, cy));
        self.direction = Direction::Right;
        self.pending.clear();
        self.place_food();
        self.last_step = Instant::now();
        self.game_over = false;
        self.score = 0;
    }

    fn paint(&self, painter: &mut Painter, bounds: Rect) {
        let play = play_rect(bounds);

        // Title bar above the playfield: light-gray strip with the score.
        let title_bar = Rect::new(bounds.x, bounds.y, bounds.w, HUD_TOP);
        painter.fill_rect(title_bar, Color::LIGHT_GRAY);
        painter.h_line(bounds.x, bounds.y + HUD_TOP - 1, bounds.w, Color::BLACK);
        let score_text = format!("SCORE  {}", self.score);
        painter.text(bounds.x + 8, bounds.y + 4, &score_text, 11.0, Color::BLACK);

        // Playfield: white canvas, snake + food drawn into it.
        painter.fill_rect(play, Color::WHITE);

        let cell_rect = |gx: i32, gy: i32| {
            Rect::new(
                play.x + self.offset_x + gx * CELL,
                play.y + self.offset_y + gy * CELL,
                CELL,
                CELL,
            )
        };

        // Food: a solid red cell with a 1px white inset for a little
        // "Win 3.1 sprite" look.
        let food = cell_rect(self.food.0, self.food.1);
        painter.fill_rect(food.inset(1), Color::RED);

        // Snake: head darker than the body so the direction is readable.
        for (i, &(x, y)) in self.body.iter().enumerate() {
            let r = cell_rect(x, y).inset(1);
            let color = if i == 0 { Color::BLACK } else { Color::NAVY };
            painter.fill_rect(r, color);
        }

        // Bottom strip: key hints, separated from the playfield by a
        // single divider line so the canvas above stays clean.
        let hint_bar = Rect::new(
            bounds.x,
            bounds.y + bounds.h - HUD_BOTTOM,
            bounds.w,
            HUD_BOTTOM,
        );
        painter.fill_rect(hint_bar, Color::LIGHT_GRAY);
        painter.h_line(bounds.x, hint_bar.y, bounds.w, Color::BLACK);
        let hint = "ESC back  \u{2022}  SPACE restart";
        painter.text(bounds.x + 8, hint_bar.y + 4, hint, 10.0, Color::DARK_GRAY);

        if self.game_over {
            // Game-over banner stays inside the playfield so it never
            // overlaps the HUD strips.
            let banner = Rect::new(play.x + play.w / 2 - 80, play.y + play.h / 2 - 18, 160, 36);
            painter.fill_rect(banner, Color::WHITE);
            painter.stroke_rect(banner, Color::BLACK);
            painter.text_centered(banner, "GAME OVER", 16.0, Color::RED);
        }
    }

    fn event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::KeyDown { key, .. } => match key {
                Key::Named(NamedKey::Left) => {
                    self.queue_direction(Direction::Left);
                }
                Key::Named(NamedKey::Right) => {
                    self.queue_direction(Direction::Right);
                }
                Key::Named(NamedKey::Up) => {
                    self.queue_direction(Direction::Up);
                }
                Key::Named(NamedKey::Down) => {
                    self.queue_direction(Direction::Down);
                }
                Key::Named(NamedKey::Space) if self.game_over => {
                    self.reset();
                    ctx.request_paint();
                }
                _ => {}
            },
            Event::Tick => {
                if self.game_over {
                    return;
                }
                let now = Instant::now();
                if now.duration_since(self.last_step) >= STEP_INTERVAL {
                    self.last_step = now;
                    self.step();
                    ctx.request_paint();
                }
            }
            _ => {}
        }
    }
}

// -------------------------------------------------------------------- sysinfo

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    const MB: f64 = KB * 1000.0;
    const GB: f64 = MB * 1000.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} kB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_binary_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} GiB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MiB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KiB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn gather_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    // pfetch's "os" field is the friendly long OS name (e.g. "macOS 26.0"),
    // which is exactly what `long_os_version` resolves to here.
    let operating_system = host::long_os_version()
        .or_else(host::os_version)
        .unwrap_or_else(|| "Unknown OS".to_string());

    let cpu = match host::cpu_brand(&sys) {
        Some(brand) => {
            let trimmed = brand.trim();
            match host::cpu_frequency_mhz(&sys) {
                Some(freq) if freq > 0 => format!("{} @ {} MHz", trimmed, freq),
                _ => trimmed.to_string(),
            }
        }
        None => "Unknown CPU".to_string(),
    };

    let memory_line = format_binary_bytes(host::total_memory_bytes(&sys));

    let disks = Disks::new_with_refreshed_list();
    let mut total_disk = 0u64;
    let mut avail_disk = 0u64;
    for disk in disks.list() {
        total_disk += disk.total_space();
        avail_disk += disk.available_space();
    }
    let disk_line = if total_disk > 0 {
        format!(
            "{} Free of {}",
            format_bytes(avail_disk),
            format_bytes(total_disk)
        )
    } else {
        "Disk information unavailable".to_string()
    };

    let uptime = seconds_to_string(System::uptime());

    // Firmware-reported hardware identity. `Product` is unimplemented on some
    // platforms (e.g. NetBSD), so treat every field as optional.
    let vendor = host::product_vendor_name().map(|v| v.trim().to_string());
    let family = host::product_family()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    // Overview headline: the friendly model name, or the short hostname when no
    // model is reported ("peregrine" rather than "peregrine.fritz.box").
    let machine = match &family {
        Some(model) => prettify_model(model),
        None => short_hostname(),
    };

    SystemInfo {
        machine,
        operating_system,
        vendor,
        model: host::product_name(),
        cpu,
        memory_line,
        disk_line,
        kernel: host::kernel_long_version(),
        uptime,
    }
}

/// First label of the hostname: `peregrine.fritz.box` → `peregrine`. Used as
/// the headline fallback where no product/model info is available.
fn short_hostname() -> String {
    match host::host_name() {
        Some(host) => host.split('.').next().unwrap_or(&host).to_string(),
        None => "Computer".to_string(),
    }
}

/// Map a raw machine model identifier to a friendly product name via a small
/// shipped table of `regex → name` rules; the first matching rule wins. An
/// identifier that matches nothing is returned unchanged, so machines we have
/// no rule for still show their reported model rather than blanking out.
fn prettify_model(model: &str) -> String {
    // Ordered most-specific first (e.g. `MacBookPro` before `MacBook`).
    const RULES: &[(&str, &str)] = &[
        (r"^MacBookPro", "MacBook Pro"),
        (r"^MacBookAir", "MacBook Air"),
        (r"^MacBook", "MacBook"),
        (r"^Macmini", "Mac mini"),
        (r"^MacStudio", "Mac Studio"),
        (r"^MacPro", "Mac Pro"),
        (r"^iMacPro", "iMac Pro"),
        (r"^iMac", "iMac"),
    ];
    for (pattern, name) in RULES {
        if Regex::new(pattern).is_ok_and(|re| re.is_match(model)) {
            return (*name).to_string();
        }
    }
    model.to_string()
}

/// Format an uptime in seconds the way pfetch does: the largest non-zero
/// units only, e.g. "1d 3h 20m" or "45m". Minutes are always shown so a
/// freshly booted machine reads "0m" rather than an empty string.
fn seconds_to_string(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    let mut result = String::new();
    if days > 0 {
        result.push_str(&format!("{}d", days));
    }
    if hours > 0 {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&format!("{}h", hours));
    }
    if minutes > 0 || result.is_empty() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&format!("{}m", minutes));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prettify_model_maps_apple_identifiers() {
        assert_eq!(prettify_model("MacBookPro18,3"), "MacBook Pro");
        assert_eq!(prettify_model("MacBookAir10,1"), "MacBook Air");
        assert_eq!(prettify_model("MacBook10,1"), "MacBook");
        assert_eq!(prettify_model("Macmini9,1"), "Mac mini");
        assert_eq!(prettify_model("iMac21,1"), "iMac");
        assert_eq!(prettify_model("iMacPro1,1"), "iMac Pro");
    }

    #[test]
    fn prettify_model_passes_through_unknown_identifiers() {
        // Lenovo machine-type codes and unmapped Apple Silicon ids stay as-is.
        assert_eq!(prettify_model("20AN"), "20AN");
        assert_eq!(prettify_model("Mac14,2"), "Mac14,2");
    }
}
