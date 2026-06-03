use sysinfo::{Product, System};

fn main() {
    println!(
        "Product::vendor_name()        = {:?}",
        Product::vendor_name()
    );
    println!("Product::name()               = {:?}", Product::name());
    println!("Product::family()             = {:?}", Product::family());
    println!("Product::version()            = {:?}", Product::version());
    println!(
        "Product::stock_keeping_unit() = {:?}",
        Product::stock_keeping_unit()
    );
    println!("---");
    println!("System::host_name()           = {:?}", System::host_name());
    println!("System::name()                = {:?}", System::name());
    println!(
        "System::long_os_version()     = {:?}",
        System::long_os_version()
    );
    println!("System::os_version()          = {:?}", System::os_version());
    println!(
        "System::distribution_id()     = {:?}",
        System::distribution_id()
    );
    println!("---");
    println!(
        "System::kernel_version()           = {:?}",
        System::kernel_version()
    );
    println!(
        "System::kernel_long_version()     = {:?}",
        System::kernel_long_version()
    );
    println!("System::os_version()          = {:?}", System::os_version());
    println!(
        "System::distribution_id()     = {:?}",
        System::distribution_id()
    );
    println!("---");
    // Decimal units, matching `diskutil` ("1.0 TB" == 1_000_555_581_440 bytes).
    let decimal = |bytes: u64| -> String {
        const TB: f64 = 1e12;
        const GB: f64 = 1e9;
        let b = bytes as f64;
        if b >= TB {
            format!("{:.2} TB", b / TB)
        } else {
            format!("{:.1} GB", b / GB)
        }
    };
    match retrofetch::disk::disk_space() {
        Some(space) => {
            println!(
                "disk::disk_space().total      = {} bytes ({})",
                space.total,
                decimal(space.total)
            );
            println!(
                "disk::disk_space().available  = {} bytes ({})",
                space.available,
                decimal(space.available)
            );
        }
        None => println!("disk::disk_space()            = unavailable"),
    }
}
