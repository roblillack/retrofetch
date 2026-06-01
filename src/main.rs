use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use saudade::EventCtx;
use saudade::{
    App, Bevel, Button, Color, Container, Event, Image, Key, Label, MouseButton, NamedKey, Painter,
    PopupRequest, Rect, Theme, Widget, WindowConfig,
};
use sysinfo::{Disks, System};

const CONTENT_WIDTH: i32 = 395;
const CONTENT_HEIGHT: i32 = 305;

const LOGO_RED: Color = Color::RED;
const LOGO_GREEN: Color = Color::GREEN;
const LOGO_BLUE: Color = Color::NAVY;
const LOGO_YELLOW: Color = Color::YELLOW;

/// Bounding box of the four-square Windows logo inside the about box,
/// in the root widget's logical coordinate system. Five clicks here
/// within `EASTER_EGG_WINDOW` swap the about chrome for a Snake game.
const LOGO_HIT: Rect = Rect::new(16, 22, 40, 28);
const EASTER_EGG_CLICKS: usize = 5;
const EASTER_EGG_WINDOW: Duration = Duration::from_secs(2);

struct SystemInfo {
    os_name: String,
    os_version: String,
    kernel: String,
    cpu: String,
    memory_line: String,
    disk_line: String,
    licensed_to: String,
    licensed_org: String,
}

fn main() {
    let info = gather_system_info();

    let about = build_about_box(&info);
    let root = AboutWithEasterEgg::new(about);

    App::new(
        WindowConfig::new("About Retrofetch", CONTENT_WIDTH, CONTENT_HEIGHT),
        root,
    )
    .with_theme(Theme::windows_31())
    .run();
}

fn build_about_box(info: &SystemInfo) -> Container {
    let mut root = Container::new(CONTENT_WIDTH, CONTENT_HEIGHT)
        .with_background(Color::WHITE)
        .with_border(Color::BLACK);

    let content_x = 4;
    let content_y = 4;

    let logo_x = content_x + 12;
    let logo_y = content_y + 18;
    root.push(build_windows_logo(logo_x, logo_y));

    // Compact labels under the logo. Their boxes start in the logo's column;
    // the text is small enough not to bleed into the system-info text on the
    // right, so a generous box width just keeps it from being clipped.
    root.push(Label::new(Rect::new(logo_x - 2, logo_y + 30, 80, 12), "MICROSOFT").with_size(8.0));
    root.push(Label::new(Rect::new(logo_x + 4, logo_y + 40, 80, 12), "WINDOWS").with_size(8.0));

    let text_x = logo_x + 56;
    // Stop the info lines short of the OK button so they share the top strip
    // without overlapping it.
    let text_w = (CONTENT_WIDTH - 78) - text_x - 6;
    let mut text_y = logo_y + 6;
    root.push(Label::new(Rect::new(text_x, text_y, text_w, 16), info.os_name.clone()));
    text_y += 14;
    root.push(Label::new(
        Rect::new(text_x, text_y, text_w, 16),
        format!("Version {}", info.os_version),
    ));
    text_y += 14;
    root.push(Label::new(
        Rect::new(text_x, text_y, text_w, 16),
        format!("Kernel {}", info.kernel),
    ));

    root.push(
        Button::new(
            Rect::new(CONTENT_WIDTH - 78, content_y + 12, 60, 22),
            "OK",
        )
        .default(true)
        .on_click(|cx| cx.close()),
    );

    let license_x = content_x + 90;
    let license_w = CONTENT_WIDTH - license_x - 6;
    let license_y = content_y + 108;
    root.push(Label::new(
        Rect::new(license_x, license_y, license_w, 16),
        "This product is licensed to:",
    ));
    root.push(Label::new(
        Rect::new(license_x, license_y + 14, license_w, 16),
        info.licensed_to.clone(),
    ));
    root.push(Label::new(
        Rect::new(license_x, license_y + 28, license_w, 16),
        info.licensed_org.clone(),
    ));

    root.push(Bevel::etched_line(
        content_x + 12,
        license_y + 60,
        CONTENT_WIDTH - 40,
    ));

    let stats_x = content_x + 22;
    let stats_w = CONTENT_WIDTH - stats_x - 6;
    let stats_y = license_y + 72;
    root.push(Label::new(
        Rect::new(stats_x, stats_y, stats_w, 16),
        format!("CPU: {}", info.cpu),
    ));
    root.push(Label::new(
        Rect::new(stats_x, stats_y + 16, stats_w, 16),
        format!("Memory: {}", info.memory_line),
    ));
    root.push(Label::new(
        Rect::new(stats_x, stats_y + 32, stats_w, 16),
        format!("Disk: {}", info.disk_line),
    ));

    root
}

/// 40×28 four-square Windows logo with a black frame — drawn procedurally so
/// retrofetch ships without any image assets.
fn build_windows_logo(x: i32, y: i32) -> Image {
    let width = 40;
    let height = 28;
    let mut img = Image::new(x, y, width, height);

    img.fill_rect(Rect::new(2, 2, 16, 10), LOGO_RED);
    img.fill_rect(Rect::new(20, 4, 16, 10), LOGO_GREEN);
    img.fill_rect(Rect::new(2, 14, 16, 10), LOGO_BLUE);
    img.fill_rect(Rect::new(20, 16, 16, 10), LOGO_YELLOW);

    for xx in 1..width - 1 {
        img.set_pixel(xx, 1, Color::BLACK);
        img.set_pixel(xx, height - 2, Color::BLACK);
    }
    for yy in 1..height - 1 {
        img.set_pixel(1, yy, Color::BLACK);
        img.set_pixel(width - 2, yy, Color::BLACK);
    }

    img
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

        let hit_wall =
            head.0 < 0 || head.0 >= self.grid_w || head.1 < 0 || head.1 >= self.grid_h;
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
            let color = if i == 0 {
                Color::BLACK
            } else {
                Color::NAVY
            };
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
            let banner = Rect::new(
                play.x + play.w / 2 - 80,
                play.y + play.h / 2 - 18,
                160,
                36,
            );
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
                Key::Named(NamedKey::Space)
                    if self.game_over => {
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

fn format_number(value: u64) -> String {
    let digits: Vec<char> = value.to_string().chars().rev().collect();
    let mut out = String::new();
    for (idx, ch) in digits.iter().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(*ch);
    }
    out.chars().rev().collect()
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn gather_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let os_name = System::name().unwrap_or_else(|| "Unknown OS".to_string());
    let os_version = System::long_os_version()
        .or_else(System::os_version)
        .unwrap_or_else(|| "Unknown Version".to_string());
    let kernel = System::kernel_version().unwrap_or_else(|| "Unknown Kernel".to_string());

    let cpu = sys
        .cpus()
        .first()
        .map(|cpu| {
            let brand = cpu.brand().trim();
            let freq = cpu.frequency();
            if freq > 0 {
                format!("{} @ {} MHz", brand, freq)
            } else {
                brand.to_string()
            }
        })
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let mem_free = sys.free_memory();
    let memory_line = format!("{} KB Free", format_number(mem_free));

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

    let licensed_to = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "User".to_string());
    let licensed_org = System::host_name().unwrap_or_else(|| "Computer".to_string());

    SystemInfo {
        os_name,
        os_version,
        kernel,
        cpu,
        memory_line,
        disk_line,
        licensed_to,
        licensed_org,
    }
}
