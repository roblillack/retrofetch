//! Renders the README screenshot straight from the real about-box widget.
//!
//! It hand-rolls a representative [`SystemInfo`] (so the rendered values are
//! the same on every machine, regardless of the OS we run the example on),
//! builds the about box via [`retrofetch::ui::build_about_box`], then drives
//! it through saudade's offscreen [`MockBackend`] with the bundled DejaVu
//! fonts — so glyph rasterization matches across hosts. The window is wrapped
//! in Canoe-style dialog chrome (title bar, frame and drop shadow) via
//! [`MockBackend::render_framed`] so the image looks the way a user sees the
//! app on the desktop. No windowing system is needed and the output is
//! byte-stable across machines. The shot is captured at 2× for crisp hi-DPI
//! rendering. Run it from the crate root to refresh `screenshot.png`:
//!
//! ```sh
//! cargo run --example screenshot
//! ```

use std::path::Path;

use retrofetch::ui::{self, SystemInfo};
use saudade::mock::MockBackend;
use saudade::{Font, Theme, Widget, WindowChrome};

/// Capture the window at 2× so the README image stays crisp on hi-DPI displays.
const SCALE: f32 = 2.0;

fn main() {
    let info = sample_info();
    let theme = Theme::windows_31();

    let sans = sans_font();
    let mono = mono_font();

    // Measure with the bundled font so the window width matches what the
    // offscreen backend will actually rasterize. `family` is the firmware
    // product family the binary uses for headline sizing — here `info.model`
    // doubles as that, since we built `info.machine` from it.
    let content_width = ui::content_width_with_font(
        &info,
        info.model.as_deref(),
        theme.font_size,
        &sans,
    );

    let mut about = ui::build_about_box(&info, content_width);
    let height = about.bounds().h;

    let backend = MockBackend::new(content_width, height)
        .with_scale(SCALE)
        .with_theme(theme)
        .with_font(sans)
        .with_mono_font(mono);

    let chrome = WindowChrome::dialog("About This Computer");
    let png = backend.render_framed(&mut about, &chrome).to_png();

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshot.png");
    std::fs::write(&path, png).expect("write screenshot");
    println!("wrote {}", path.display());
}

/// A representative MacBook so every detail row has content and the Apple
/// logo branch of [`ui::logo_svg_for`] is exercised.
fn sample_info() -> SystemInfo {
    SystemInfo {
        machine: "MacBook Pro".to_string(),
        operating_system: "macOS 26.1".to_string(),
        vendor: Some("Apple Inc.".to_string()),
        model: Some("MacBookPro18,3".to_string()),
        cpu: "Apple M1 Max @ 3228 MHz".to_string(),
        memory_line: "32 GiB".to_string(),
        disk_line: "2.0 TB (1.4 TB free)".to_string(),
        kernel: "Darwin 25.1.0".to_string(),
        window_manager: Some("Quartz Compositor".to_string()),
        packages: Some(213),
        uptime: "3d 14h 22m".to_string(),
        distribution_id: "macos".to_string(),
    }
}

fn sans_font() -> Font {
    Font::from_bytes(include_bytes!("../tests/fonts/DejaVuSans.ttf").to_vec())
        .expect("bundled DejaVuSans.ttf failed to load")
}

fn mono_font() -> Font {
    Font::from_bytes(include_bytes!("../tests/fonts/DejaVuSansMono.ttf").to_vec())
        .expect("bundled DejaVuSansMono.ttf failed to load")
}
