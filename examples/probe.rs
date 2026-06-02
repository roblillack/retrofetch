use retrofetch::host;
use sysinfo::{Product, System};

fn main() {
    // Hardware identity. Vendor / name / family route through `host` so the
    // workaround for platforms `sysinfo` doesn't support (OpenBSD) kicks in.
    println!(
        "host::product_vendor_name()   = {:?}",
        host::product_vendor_name()
    );
    println!("host::product_name()          = {:?}", host::product_name());
    println!(
        "host::product_family()        = {:?}",
        host::product_family()
    );
    println!("Product::version()            = {:?}", Product::version());
    println!(
        "Product::stock_keeping_unit() = {:?}",
        Product::stock_keeping_unit()
    );
    println!("---");
    println!("host::host_name()             = {:?}", host::host_name());
    println!("System::name()                = {:?}", System::name());
    println!(
        "host::long_os_version()       = {:?}",
        host::long_os_version()
    );
    println!("host::os_version()            = {:?}", host::os_version());
    println!(
        "System::distribution_id()     = {:?}",
        System::distribution_id()
    );
    println!("---");
    println!(
        "System::kernel_version()      = {:?}",
        System::kernel_version()
    );
    println!(
        "host::kernel_long_version()   = {:?}",
        host::kernel_long_version()
    );
}
