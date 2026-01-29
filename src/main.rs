use font8x8::UnicodeFonts;
use sysinfo::{Disks, System};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

const CONTENT_WIDTH: i32 = 395;
const CONTENT_HEIGHT: i32 = 305;

const COLOR_BLACK: u32 = 0xFF000000;
const COLOR_WHITE: u32 = 0xFFFFFFFF;
const COLOR_GRAY: u32 = 0xFFC0C0C0;
const COLOR_DARK_GRAY: u32 = 0xFF808080;
const COLOR_LIGHT_GRAY: u32 = 0xFFE0E0E0;
const COLOR_BLUE: u32 = 0xFF000080;
const COLOR_RED: u32 = 0xFFCC0000;
const COLOR_GREEN: u32 = 0xFF00A000;
const COLOR_YELLOW: u32 = 0xFFCCCC00;

#[derive(Clone)]
struct Label {
    x: i32,
    y: i32,
    text: String,
    color: u32,
}

#[derive(Clone)]
struct Icon {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pixels: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Clone)]
struct Button {
    rect: Rect,
    text: String,
}

struct Ui {
    labels: Vec<Label>,
    icons: Vec<Icon>,
    buttons: Vec<Button>,
}

impl Ui {
    fn new() -> Self {
        Self {
            labels: Vec::new(),
            icons: Vec::new(),
            buttons: Vec::new(),
        }
    }

    fn draw(&self, canvas: &mut Canvas) {
        for icon in &self.icons {
            canvas.draw_icon(icon);
        }
        for label in &self.labels {
            canvas.draw_text(label.x, label.y, &label.text, label.color);
        }
        for button in &self.buttons {
            canvas.draw_button(button.rect, &button.text);
        }
    }
}

struct Canvas {
    width: i32,
    height: i32,
    pixels: Vec<u32>,
}

impl Canvas {
    fn new(width: i32, height: i32) -> Self {
        let len = (width.max(1) * height.max(1)) as usize;
        Self {
            width,
            height,
            pixels: vec![COLOR_WHITE; len],
        }
    }

    fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let idx = (y * self.width + x) as usize;
        self.pixels[idx] = color;
    }

    fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);
        for yy in y.max(0)..y_end {
            let row = (yy * self.width) as usize;
            for xx in x.max(0)..x_end {
                self.pixels[row + xx as usize] = color;
            }
        }
    }

    fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        for xx in x..(x + w) {
            self.set_pixel(xx, y, color);
            self.set_pixel(xx, y + h - 1, color);
        }
        for yy in y..(y + h) {
            self.set_pixel(x, yy, color);
            self.set_pixel(x + w - 1, yy, color);
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: u32) {
        let mut cursor_x = x;
        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = x;
                continue;
            }
            if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
                for (row, bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if (bits >> col) & 1 == 1 {
                            self.set_pixel(cursor_x + col as i32, y + row as i32, color);
                        }
                    }
                }
            }
            cursor_x += 8;
        }
    }

    fn draw_icon(&mut self, icon: &Icon) {
        for yy in 0..icon.height {
            for xx in 0..icon.width {
                let idx = (yy * icon.width + xx) as usize;
                let color = icon.pixels[idx];
                if (color >> 24) == 0 {
                    continue;
                }
                self.set_pixel(icon.x + xx, icon.y + yy, color);
            }
        }
    }

    fn draw_button(&mut self, rect: Rect, text: &str) {
        self.draw_rect(rect.x, rect.y, rect.w, rect.h, COLOR_GRAY);

        self.draw_rect_outline(rect.x, rect.y, rect.w, rect.h, COLOR_BLACK);
        self.draw_rect_outline(rect.x + 1, rect.y + 1, rect.w - 2, rect.h - 2, COLOR_LIGHT_GRAY);
        self.draw_rect_outline(rect.x + 2, rect.y + 2, rect.w - 4, rect.h - 4, COLOR_DARK_GRAY);

        let text_width = (text.chars().count() as i32) * 8;
        let text_x = rect.x + (rect.w - text_width) / 2;
        let text_y = rect.y + (rect.h - 8) / 2;
        self.draw_text(text_x, text_y, text, COLOR_BLACK);
    }

    fn blit_scaled_to(&self, target: &mut [u32], target_w: i32, target_h: i32) {
        let src_w = self.width.max(1) as f64;
        let src_h = self.height.max(1) as f64;
        let dst_w = target_w.max(1) as f64;
        let dst_h = target_h.max(1) as f64;
        let scale_x = src_w / dst_w;
        let scale_y = src_h / dst_h;

        for y in 0..target_h.max(1) {
            let src_y = (y as f64 * scale_y).floor() as i32;
            let src_y = src_y.clamp(0, self.height.max(1) - 1);
            let src_row = (src_y * self.width.max(1)) as usize;
            let dst_row = (y * target_w.max(1)) as usize;

            for x in 0..target_w.max(1) {
                let src_x = (x as f64 * scale_x).floor() as i32;
                let src_x = src_x.clamp(0, self.width.max(1) - 1);
                let src_idx = src_row + src_x as usize;
                let dst_idx = dst_row + x as usize;
                target[dst_idx] = self.pixels[src_idx];
            }
        }
    }
}

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

fn format_number(value: u64) -> String {
    let digits = value.to_string().chars().rev().collect::<Vec<_>>();
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
    let mem_line = format!("{} KB Free", format_number(mem_free));

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
        memory_line: mem_line,
        disk_line,
        licensed_to,
        licensed_org,
    }
}

fn build_windows_logo() -> Icon {
    let width = 40;
    let height = 28;
    let mut pixels = vec![0u32; (width * height) as usize];

    let mut set_rect = |x: i32, y: i32, w: i32, h: i32, color: u32| {
        for yy in y..(y + h) {
            for xx in x..(x + w) {
                if xx < 0 || yy < 0 || xx >= width || yy >= height {
                    continue;
                }
                let idx = (yy * width + xx) as usize;
                pixels[idx] = color;
            }
        }
    };

    set_rect(2, 2, 16, 10, COLOR_RED);
    set_rect(20, 4, 16, 10, COLOR_GREEN);
    set_rect(2, 14, 16, 10, COLOR_BLUE);
    set_rect(20, 16, 16, 10, COLOR_YELLOW);

    for xx in 1..(width - 1) {
        set_rect(xx, 1, 1, 1, COLOR_BLACK);
        set_rect(xx, height - 2, 1, 1, COLOR_BLACK);
    }
    for yy in 1..(height - 1) {
        set_rect(1, yy, 1, 1, COLOR_BLACK);
        set_rect(width - 2, yy, 1, 1, COLOR_BLACK);
    }

    Icon {
        x: 0,
        y: 0,
        width,
        height,
        pixels,
    }
}

fn build_ui(info: &SystemInfo) -> (Ui, Rect) {
    let mut ui = Ui::new();

    let content_x = 4;
    let content_y = 4;

    let logo_x = content_x + 12;
    let logo_y = content_y + 18;
    let mut logo = build_windows_logo();
    logo.x = logo_x;
    logo.y = logo_y;
    ui.icons.push(logo);

    ui.labels.push(Label {
        x: logo_x + 6,
        y: logo_y + 34,
        text: "MICROSOFT".to_string(),
        color: COLOR_BLACK,
    });
    ui.labels.push(Label {
        x: logo_x + 8,
        y: logo_y + 44,
        text: "WINDOWS".to_string(),
        color: COLOR_BLACK,
    });

    let text_x = logo_x + 64;
    let mut text_y = logo_y + 6;

    ui.labels.push(Label {
        x: text_x,
        y: text_y,
        text: info.os_name.clone(),
        color: COLOR_BLACK,
    });
    text_y += 14;
    ui.labels.push(Label {
        x: text_x,
        y: text_y,
        text: format!("Version {}", info.os_version),
        color: COLOR_BLACK,
    });
    text_y += 14;
    ui.labels.push(Label {
        x: text_x,
        y: text_y,
        text: format!("Kernel {}", info.kernel),
        color: COLOR_BLACK,
    });

    let button_rect = Rect {
        x: CONTENT_WIDTH - 78,
        y: content_y + 12,
        w: 60,
        h: 20,
    };
    ui.buttons.push(Button {
        rect: button_rect,
        text: "OK".to_string(),
    });

    let license_y = content_y + 108;
    ui.labels.push(Label {
        x: content_x + 90,
        y: license_y,
        text: "This product is licensed to:".to_string(),
        color: COLOR_BLACK,
    });
    ui.labels.push(Label {
        x: content_x + 90,
        y: license_y + 14,
        text: info.licensed_to.clone(),
        color: COLOR_BLACK,
    });
    ui.labels.push(Label {
        x: content_x + 90,
        y: license_y + 28,
        text: info.licensed_org.clone(),
        color: COLOR_BLACK,
    });

    let line_icon = Icon {
        x: content_x + 12,
        y: license_y + 54,
        width: CONTENT_WIDTH - 40,
        height: 1,
        pixels: vec![COLOR_BLACK; (CONTENT_WIDTH - 40) as usize],
    };
    ui.icons.push(line_icon);

    let stats_y = license_y + 72;
    ui.labels.push(Label {
        x: content_x + 22,
        y: stats_y,
        text: format!("CPU: {}", info.cpu),
        color: COLOR_BLACK,
    });
    ui.labels.push(Label {
        x: content_x + 22,
        y: stats_y + 16,
        text: format!("Memory: {}", info.memory_line),
        color: COLOR_BLACK,
    });
    ui.labels.push(Label {
        x: content_x + 22,
        y: stats_y + 32,
        text: format!("Disk: {}", info.disk_line),
        color: COLOR_BLACK,
    });

    (ui, button_rect)
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let window = WindowBuilder::new()
        .with_title("About Retrofetch")
        .with_inner_size(LogicalSize::new(CONTENT_WIDTH as f64, CONTENT_HEIGHT as f64))
        .with_resizable(false)
        .build(&event_loop)
        .expect("window");

    let window = Rc::new(window);
    let context =
        softbuffer::Context::new(window.clone()).expect("softbuffer context");
    let mut surface =
        softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
    let mut size = window.inner_size();
    let mut canvas = Canvas::new(CONTENT_WIDTH, CONTENT_HEIGHT);

    let info = gather_system_info();
    let (ui, ok_button) = build_ui(&info);
    let mut cursor_pos = (0i32, 0i32);
    let mut needs_redraw = true;

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::RedrawRequested => {
                    canvas.clear(COLOR_WHITE);

                    canvas.draw_rect(0, 0, CONTENT_WIDTH, CONTENT_HEIGHT, COLOR_WHITE);
                    canvas.draw_rect_outline(
                        2,
                        2,
                        CONTENT_WIDTH - 4,
                        CONTENT_HEIGHT - 4,
                        COLOR_BLACK,
                    );

                    ui.draw(&mut canvas);

                    let mut buffer = surface.buffer_mut().expect("buffer");
                    canvas.blit_scaled_to(&mut buffer, size.width as i32, size.height as i32);
                    buffer.present().expect("present");
                    needs_redraw = false;
                }
                WindowEvent::Resized(new_size) => {
                    size = new_size;
                    let width = NonZeroU32::new(size.width.max(1)).expect("width");
                    let height = NonZeroU32::new(size.height.max(1)).expect("height");
                    surface.resize(width, height).expect("resize");
                    needs_redraw = true;
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_pos = (position.x as i32, position.y as i32);
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    if cursor_pos.0 >= ok_button.x
                        && cursor_pos.0 <= ok_button.x + ok_button.w
                        && cursor_pos.1 >= ok_button.y
                        && cursor_pos.1 <= ok_button.y + ok_button.h
                    {
                        elwt.exit();
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                if needs_redraw {
                    window.request_redraw();
                }
            }
            _ => {}
        }
        })
        .expect("event loop");
}
