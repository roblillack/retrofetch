//! Compile-time shim around the parts of `sysinfo` that aren't implemented on
//! OpenBSD. On every other supported platform these delegate straight to
//! `sysinfo`; on OpenBSD they shell out to `uname` / `sysctl(8)` instead so the
//! about-box doesn't end up showing "Unknown" for every Software/Hardware row.

#[cfg(not(target_os = "openbsd"))]
pub use native::*;

#[cfg(target_os = "openbsd")]
pub use openbsd::*;

#[cfg(not(target_os = "openbsd"))]
mod native {
    use sysinfo::{Product, System};

    pub fn long_os_version() -> Option<String> {
        System::long_os_version()
    }
    pub fn os_version() -> Option<String> {
        System::os_version()
    }
    pub fn host_name() -> Option<String> {
        System::host_name()
    }
    pub fn kernel_long_version() -> String {
        System::kernel_long_version()
    }
    pub fn product_vendor_name() -> Option<String> {
        Product::vendor_name()
    }
    pub fn product_name() -> Option<String> {
        Product::name()
    }
    pub fn product_family() -> Option<String> {
        Product::family()
    }
    pub fn cpu_brand(sys: &System) -> Option<String> {
        sys.cpus().first().map(|c| c.brand().to_string())
    }
    pub fn cpu_frequency_mhz(sys: &System) -> Option<u64> {
        sys.cpus().first().map(|c| c.frequency())
    }
}

#[cfg(target_os = "openbsd")]
mod openbsd {
    use std::process::Command;

    fn run(cmd: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(cmd).args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8(out.stdout).ok()?;
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn uname(flag: &str) -> Option<String> {
        run("/usr/bin/uname", &[flag])
    }
    fn sysctl(key: &str) -> Option<String> {
        run("/sbin/sysctl", &["-n", key])
    }

    pub fn long_os_version() -> Option<String> {
        let sys = uname("-s")?;
        let rel = uname("-r")?;
        Some(format!("{sys} {rel}"))
    }
    pub fn os_version() -> Option<String> {
        uname("-r")
    }
    pub fn host_name() -> Option<String> {
        uname("-n")
    }
    pub fn kernel_long_version() -> String {
        long_os_version().unwrap_or_else(|| "Unknown".to_string())
    }
    // OpenBSD's firmware identity lives under `hw.*`:
    //   hw.vendor  -> "LENOVO"                 (Product::vendor_name)
    //   hw.version -> "ThinkPad X1 Carbon 7th" (Product::family — the
    //                 friendly product name used as the Overview headline)
    //   hw.product -> "20QDCTO1WW"             (Product::name — raw id)
    pub fn product_vendor_name() -> Option<String> {
        sysctl("hw.vendor")
    }
    pub fn product_name() -> Option<String> {
        sysctl("hw.product")
    }
    pub fn product_family() -> Option<String> {
        sysctl("hw.version")
    }
    // sysinfo's CPU enumeration is empty on OpenBSD, so fall back to sysctl:
    //   hw.model    -> "Intel(R) Core(TM) i7-8565U CPU @ 1.80GHz"
    //   hw.cpuspeed -> current core frequency in MHz
    pub fn cpu_brand(_sys: &sysinfo::System) -> Option<String> {
        sysctl("hw.model")
    }
    pub fn cpu_frequency_mhz(_sys: &sysinfo::System) -> Option<u64> {
        sysctl("hw.cpuspeed")?.parse().ok()
    }
}
