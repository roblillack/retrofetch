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
    pub fn total_memory_bytes(sys: &System) -> u64 {
        sys.total_memory()
    }
    // Summed (total_bytes, available_bytes) across mounted filesystems — what
    // the about-box's Disk line consumes. The number tracks filesystem space,
    // which on macOS/Linux is effectively the installed disk capacity too.
    pub fn disk_totals() -> (u64, u64) {
        use sysinfo::Disks;
        let mut total = 0u64;
        let mut avail = 0u64;
        for disk in Disks::new_with_refreshed_list().list() {
            total += disk.total_space();
            avail += disk.available_space();
        }
        (total, avail)
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
    // hw.physmem -> total physical memory in bytes (64-bit on modern OpenBSD)
    pub fn total_memory_bytes(_sys: &sysinfo::System) -> u64 {
        sysctl("hw.physmem")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
    // Filesystem capacity from `df -lkP` (POSIX 1K-block output, local mounts
    // only). Lines whose first column doesn't start with `/dev/` are skipped to
    // drop mfs/tmpfs/procfs etc.
    fn fs_disks() -> Vec<(u64, u64)> {
        let Some(text) = run("/bin/df", &["-lkP"]) else {
            return Vec::new();
        };
        text.lines()
            .skip(1)
            .filter_map(|line| {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() < 6 || !cols[0].starts_with("/dev/") {
                    return None;
                }
                let total: u64 = cols[1].parse().ok()?;
                let avail: u64 = cols[3].parse().ok()?;
                Some((total * 1024, avail * 1024))
            })
            .collect()
    }

    /// Sum of the *physical* disk capacities the kernel saw at boot, read from
    /// `/var/run/dmesg.boot`. Two categories are excluded:
    ///
    ///  - softraid(4) virtual disks (vendor "OPENBSD", product "SR …") — they
    ///    layer over a real disk that's already counted.
    ///  - USB mass-storage drives — detected by walking `sdN → scsibusM →
    ///    umassK` in the attach lines. Reliable for any device using the
    ///    standard umass(4) driver, which covers essentially all USB sticks
    ///    and external HDDs/SSDs on OpenBSD.
    ///
    /// Returns 0 when the file is missing or unparseable.
    fn physical_disk_total_bytes() -> u64 {
        use std::collections::{HashMap, HashSet};

        let Ok(text) = std::fs::read_to_string("/var/run/dmesg.boot") else {
            return 0;
        };

        // dmesg.boot can replay attachments across rescans and the kernel may
        // renumber scsibusN between dumps, so every map below uses "last seen
        // wins" — the final pass through the file reflects the running state.
        let mut scsibus_parent: HashMap<String, String> = HashMap::new();
        let mut disk_scsibus: HashMap<String, String> = HashMap::new();
        let mut softraid: HashSet<String> = HashSet::new();
        let mut sizes: HashMap<String, u64> = HashMap::new();

        for line in text.lines() {
            let dev = line.split([' ', ':']).next().unwrap_or("");

            // "scsibusN at <parent>: ..."  → record the bus's parent device.
            if dev.starts_with("scsibus")
                && let Some((_, after_at)) = line.split_once(" at ")
                && let Some(parent) = after_at.split([':', ' ']).next()
                && !parent.is_empty()
            {
                scsibus_parent.insert(dev.to_string(), parent.to_string());
            }

            // "sdN at scsibusM ...: <vendor, product, ...>"  → bus link
            // + softraid detection in the same line.
            if (dev.starts_with("sd") || dev.starts_with("wd"))
                && let Some((_, after_at)) = line.split_once(" at ")
            {
                if let Some(parent) = after_at.split([':', ' ']).next()
                    && parent.starts_with("scsibus")
                {
                    disk_scsibus.insert(dev.to_string(), parent.to_string());
                }
                if line.contains("<OPENBSD, SR ") {
                    softraid.insert(dev.to_string());
                }
            }

            // "sdN: NNNNMB, NN bytes/sector, NNNN sectors" → byte total via
            // sectors × bytes/sector (more accurate than the rounded MB).
            if (dev.starts_with("sd") || dev.starts_with("wd") || dev.starts_with("nvme"))
                && let Some((_, rest)) = line.split_once(": ")
                && rest.ends_with("sectors")
            {
                let parts: Vec<&str> = rest.split(", ").collect();
                if parts.len() >= 3
                    && let Some(bps) = parts[1].split_whitespace().next()
                    && let Some(sec) = parts[2].split_whitespace().next()
                    && let (Ok(bps), Ok(sec)) = (bps.parse::<u64>(), sec.parse::<u64>())
                {
                    sizes.insert(dev.to_string(), bps * sec);
                }
            }
        }

        let is_usb = |dev: &str| -> bool {
            disk_scsibus
                .get(dev)
                .and_then(|bus| scsibus_parent.get(bus))
                .is_some_and(|parent| parent.starts_with("umass"))
        };

        sizes
            .iter()
            .filter(|(dev, _)| !softraid.contains(*dev) && !is_usb(dev))
            .map(|(_, &size)| size)
            .sum()
    }

    /// (total, available) for the about-box's Disk row. `total` is the sum of
    /// physical disk capacities — the actually-installed storage, not just
    /// what's mounted — and falls back to filesystem totals if dmesg.boot can't
    /// be read. `available` stays filesystem-derived since there's no
    /// disk-level free-space concept once partitions are carved out.
    pub fn disk_totals() -> (u64, u64) {
        let mut fs_total = 0u64;
        let mut fs_avail = 0u64;
        for (t, a) in fs_disks() {
            fs_total += t;
            fs_avail += a;
        }
        let physical = physical_disk_total_bytes();
        let total = if physical > 0 { physical } else { fs_total };
        (total, fs_avail)
    }
}
