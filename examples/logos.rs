//! Renders every bundled OS logo in a 3-row grid, to eyeball the
//! compile-time-baked `include_svg!` marks side by side.
//!
//! Run with `cargo run --example logos`.

use saudade::{
    App, Color, Container, Painter, Rect, SvgImage, Theme, Widget, WindowConfig, include_svg,
};

/// Logo box edge, in logical pixels.
const LOGO: i32 = 72;
/// Grid cell (logo on top, name underneath).
const CELL_W: i32 = 110;
const CELL_H: i32 = 104;
/// Pixels above the logo and below it before the name.
const PAD_TOP: i32 = 6;
const LABEL_H: i32 = 16;
/// Outer margin around the whole grid.
const MARGIN: i32 = 20;
/// Columns; with 14 logos this lays out as 5 + 5 + 4 over three rows.
const COLS: usize = 5;

/// One grid cell: the baked logo drawn aspect-fit at the top, its name centered
/// below. `include_svg!` paths resolve from the crate root (CARGO_MANIFEST_DIR).
struct LogoCell {
    rect: Rect,
    image: SvgImage,
    name: &'static str,
}

impl Widget for LogoCell {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn paint(&mut self, painter: &mut Painter, _theme: &Theme) {
        let logo = Rect::new(
            self.rect.x + (self.rect.w - LOGO) / 2,
            self.rect.y + PAD_TOP,
            LOGO,
            LOGO,
        );
        painter.draw_svg(&self.image, logo);

        let label = Rect::new(self.rect.x, logo.bottom() + 2, self.rect.w, LABEL_H);
        painter.text_centered(label, self.name, 11.0, Color::BLACK);
    }
}

fn main() {
    // (display name, baked logo) — same set retrofetch picks from in
    // `logo_svg_for`, ordered to fill three rows.
    // tux.svg uses SVG features (a filter, group opacity) that `include_svg!`
    // can't bake; the macro flags this via a `deprecated` warning we can't act on.
    #[allow(deprecated)]
    let logos: [(&str, SvgImage); 14] = [
        ("Apple", include_svg!("assets/os/apple.svg")),
        ("Arch", include_svg!("assets/os/arch.svg")),
        ("Tux", include_svg!("assets/os/tux.svg")),
        ("OpenBSD", include_svg!("assets/os/openbsd.svg")),
        ("Ubuntu", include_svg!("assets/os/ubuntu.svg")),
        ("Debian", include_svg!("assets/os/debian.svg")),
        ("Fedora", include_svg!("assets/os/fedora.svg")),
        ("openSUSE", include_svg!("assets/os/opensuse.svg")),
        ("NixOS", include_svg!("assets/os/nixos.svg")),
        ("FreeBSD", include_svg!("assets/os/freebsd.svg")),
        ("NetBSD", include_svg!("assets/os/netbsd.svg")),
        ("Manjaro", include_svg!("assets/os/manjaro.svg")),
        ("Linux Mint", include_svg!("assets/os/linuxmint.svg")),
        ("Windows", include_svg!("assets/os/windows.svg")),
    ];

    let rows = logos.len().div_ceil(COLS) as i32;
    let width = 2 * MARGIN + COLS as i32 * CELL_W;
    let height = 2 * MARGIN + rows * CELL_H;

    let mut root = Container::new(width, height).with_background(Color::WHITE);
    for (i, (name, image)) in logos.into_iter().enumerate() {
        let col = (i % COLS) as i32;
        let row = (i / COLS) as i32;
        let rect = Rect::new(MARGIN + col * CELL_W, MARGIN + row * CELL_H, CELL_W, CELL_H);
        root.push(LogoCell { rect, image, name });
    }

    App::new(
        WindowConfig::new("retrofetch — OS logos", width, height),
        root,
    )
    .with_theme(Theme::windows_31())
    .run();
}
