use retrogui::{
    App, Bevel, Button, Color, Container, Image, Label, Rect, Theme, WindowConfig,
};
use sysinfo::{Disks, System};

const CONTENT_WIDTH: i32 = 395;
const CONTENT_HEIGHT: i32 = 305;

const LOGO_RED: Color = Color::RED;
const LOGO_GREEN: Color = Color::GREEN;
const LOGO_BLUE: Color = Color::NAVY;
const LOGO_YELLOW: Color = Color::YELLOW;

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

    let root = build_about_box(&info);

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

    // Compact labels under the logo — small enough to stay within the logo's
    // 40px column so they don't bleed into the system-info text on the right.
    root.push(Label::new(logo_x - 2, logo_y + 30, "MICROSOFT").with_size(8.0));
    root.push(Label::new(logo_x + 4, logo_y + 40, "WINDOWS").with_size(8.0));

    let text_x = logo_x + 56;
    let mut text_y = logo_y + 6;
    root.push(Label::new(text_x, text_y, info.os_name.clone()));
    text_y += 14;
    root.push(Label::new(
        text_x,
        text_y,
        format!("Version {}", info.os_version),
    ));
    text_y += 14;
    root.push(Label::new(
        text_x,
        text_y,
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

    let license_y = content_y + 108;
    root.push(Label::new(
        content_x + 90,
        license_y,
        "This product is licensed to:",
    ));
    root.push(Label::new(
        content_x + 90,
        license_y + 14,
        info.licensed_to.clone(),
    ));
    root.push(Label::new(
        content_x + 90,
        license_y + 28,
        info.licensed_org.clone(),
    ));

    root.push(Bevel::etched_line(
        content_x + 12,
        license_y + 60,
        CONTENT_WIDTH - 40,
    ));

    let stats_y = license_y + 72;
    root.push(Label::new(
        content_x + 22,
        stats_y,
        format!("CPU: {}", info.cpu),
    ));
    root.push(Label::new(
        content_x + 22,
        stats_y + 16,
        format!("Memory: {}", info.memory_line),
    ));
    root.push(Label::new(
        content_x + 22,
        stats_y + 32,
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
