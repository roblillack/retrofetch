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
}
