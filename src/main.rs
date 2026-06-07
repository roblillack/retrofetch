use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use retrofetch::host;
use retrofetch::ui::{self, SystemInfo};
use saudade::EventCtx;
use saudade::{
    App, Color, Container, Event, Key, MouseButton, NamedKey, Painter, PopupRequest, Rect, Theme,
    Widget, WindowConfig,
};
use sysinfo::System;

/// Five clicks on the system logo within `EASTER_EGG_WINDOW` swap the about
/// chrome for a Snake game.
const EASTER_EGG_CLICKS: usize = 5;
const EASTER_EGG_WINDOW: Duration = Duration::from_secs(2);

fn main() {
    let info = gather_system_info();
    let theme = Theme::windows_31();
    let content_width =
        ui::compute_content_width(&info, host::product_family().as_deref(), theme.font_size);

    let about = ui::build_about_box(&info, content_width);
    // The box sizes its own height from the stacked group boxes; match the
    // window to it so there's no dead strip at the bottom.
    let height = about.bounds().h;
    let root = AboutWithEasterEgg::new(about);

    App::new(
        WindowConfig::new("About This Computer", content_width, height),
        root,
    )
    .with_theme(theme)
    .run();
}

/// Root widget. Normally a transparent wrapper around the about-box
/// container — but five clicks on the system logo within two seconds
/// flip the window into Snake mode: the chrome disappears, the surface
/// turns white, and the existing window becomes the game's playfield.
struct AboutWithEasterEgg {
    about: Container,
    snake: Option<SnakeGame>,
    /// Timestamps of recent left-button presses inside `ui::LOGO_HIT`.
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
            && ui::LOGO_HIT.contains(*pos)
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

/// Format `value` with one decimal place, but trim a trailing `.0` so exact
/// values read as e.g. "16 GiB" rather than "16.0 GiB" while non-round values
/// keep their precision ("1.5 TB").
fn format_quantity(value: f64, unit: &str) -> String {
    let s = format!("{:.1}", value);
    let trimmed = s.strip_suffix(".0").unwrap_or(&s);
    format!("{} {}", trimmed, unit)
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    const MB: f64 = KB * 1000.0;
    const GB: f64 = MB * 1000.0;
    const TB: f64 = GB * 1000.0;
    const PB: f64 = TB * 1000.0;
    let bytes_f = bytes as f64;
    if bytes_f >= PB {
        format_quantity(bytes_f / PB, "PB")
    } else if bytes_f >= TB {
        format_quantity(bytes_f / TB, "TB")
    } else if bytes_f >= GB {
        format_quantity(bytes_f / GB, "GB")
    } else if bytes_f >= MB {
        format_quantity(bytes_f / MB, "MB")
    } else if bytes_f >= KB {
        format_quantity(bytes_f / KB, "kB")
    } else {
        format!("{} B", bytes)
    }
}

/// Round a physical-memory byte count up to the nearest plausible installed
/// size, smoothing out the small BIOS/iGPU reservations that make `hw.physmem`
/// (and the equivalents on other OSes) report e.g. 15.79 GiB on a 16 GiB
/// machine. Above 4 GiB, snaps to the next 2 GiB boundary; above 2 GiB, to the
/// next 1 GiB boundary; smaller values are returned unchanged so tiny systems
/// aren't misrepresented.
fn round_installed_memory(bytes: u64) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    if bytes > 4 * GIB {
        bytes.div_ceil(2 * GIB) * 2 * GIB
    } else if bytes > 2 * GIB {
        bytes.div_ceil(GIB) * GIB
    } else {
        bytes
    }
}

fn format_binary_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    const PB: f64 = TB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= PB {
        format_quantity(bytes_f / PB, "PiB")
    } else if bytes_f >= TB {
        format_quantity(bytes_f / TB, "TiB")
    } else if bytes_f >= GB {
        format_quantity(bytes_f / GB, "GiB")
    } else if bytes_f >= MB {
        format_quantity(bytes_f / MB, "MiB")
    } else if bytes_f >= KB {
        format_quantity(bytes_f / KB, "KiB")
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

    let memory_line = format_binary_bytes(round_installed_memory(host::total_memory_bytes(&sys)));

    let (total_disk, avail_disk) = host::disk_totals();
    let disk_line = if total_disk > 0 {
        format!(
            "{} ({} free)",
            format_bytes(total_disk),
            format_bytes(avail_disk)
        )
    } else {
        "Disk information unavailable".to_string()
    };

    let uptime = seconds_to_string(host::uptime_seconds());

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
        window_manager: host::window_manager(),
        packages: host::installed_package_count(),
        uptime,
        distribution_id: System::distribution_id(),
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

    #[test]
    fn round_installed_memory_snaps_to_plausible_sizes() {
        const GIB: u64 = 1024 * 1024 * 1024;
        // 16 GiB ThinkPad reporting 15.79 GiB after iGPU/BIOS reservations.
        assert_eq!(round_installed_memory(16_959_840_256), 16 * GIB);
        // Exact sizes pass through.
        assert_eq!(round_installed_memory(16 * GIB), 16 * GIB);
        assert_eq!(round_installed_memory(8 * GIB), 8 * GIB);
        // 6 GiB (4+2) stays at 6 once snapped to the 2 GiB grid.
        assert_eq!(round_installed_memory(6 * GIB - 200 * 1024 * 1024), 6 * GIB);
        // 3 GiB uses the 1 GiB grid (between 2 and 4 GiB).
        assert_eq!(round_installed_memory(3 * GIB - 100 * 1024 * 1024), 3 * GIB);
        // Small systems aren't rounded.
        assert_eq!(round_installed_memory(900_000_000), 900_000_000);
    }
}
