//! Compile-time shim around the parts of `sysinfo` that are missing or wrong on
//! some platforms. Most rows delegate straight to `sysinfo`; the exceptions:
//!
//!  - **OpenBSD**: `sysinfo` implements almost nothing, so every row shells out
//!    to `uname` / `sysctl(8)` / `df` and reads `dmesg.boot` for disk sizes.
//!  - **macOS**: `sysinfo` reports the boot disk as several APFS volumes that
//!    each claim the whole shared container, so summing them double-counts.
//!    [`disk_totals`] instead reads the *physical* drive capacity from IOKit.

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
    pub fn uptime_seconds() -> u64 {
        System::uptime()
    }
    pub fn installed_package_count() -> Option<u32> {
        // sysinfo has no package-manager integration, and the way to count
        // packages varies per distro/OS. Leaving this unsupported until each
        // platform has a known cheap path.
        None
    }
    // Summed (total_bytes, available_bytes) across mounted filesystems — what
    // the about-box's Disk line consumes. The number tracks filesystem space,
    // which on Linux/Windows is effectively the installed disk capacity too.
    #[cfg(not(target_os = "macos"))]
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

    // macOS double-counts under the naive sum (see module docs), so defer to
    // the IOKit-based physical-capacity reader.
    #[cfg(target_os = "macos")]
    pub fn disk_totals() -> (u64, u64) {
        super::macos::disk_totals()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    //! macOS disk capacity.
    //!
    //! `total` is the *physical* drive capacity read from IOKit, so it matches
    //! Disk Utility / `diskutil` and is immune to APFS volume double-counting.
    //! `available` is the boot volume's free space from `statfs("/")` — the
    //! free figure a user can actually fill (APFS volumes in one container
    //! share this pool, so reading one volume is enough).

    /// (total, available) for the about-box Disk line. Falls back to the boot
    /// volume's own size if IOKit can't be read.
    pub fn disk_totals() -> (u64, u64) {
        let (fs_total, available) = root_space();
        let physical = physical_disk_total();
        let total = if physical > 0 { physical } else { fs_total };
        (total, available)
    }

    /// (total_bytes, available_bytes) of the filesystem mounted at `/`.
    fn root_space() -> (u64, u64) {
        let mut buf = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: "/" is a valid C string; `statfs` fully initialises `buf`
        // when it returns 0.
        if unsafe { libc::statfs(c"/".as_ptr(), buf.as_mut_ptr()) } != 0 {
            return (0, 0);
        }
        let buf = unsafe { buf.assume_init() };
        let block = buf.f_bsize as u64;
        (buf.f_blocks * block, buf.f_bavail * block)
    }

    /// Sum the capacity of every physical drive via IOKit.
    ///
    /// A physical drive is a *whole* `IOMedia` that is non-removable and whose
    /// content is a partition scheme (`*_partition_scheme`). That selects the
    /// SSD/HDD itself while excluding:
    ///   - partitions (not whole);
    ///   - APFS / CoreStorage containers (whole, but their content is a
    ///     synthesized container GUID, not a partition scheme);
    ///   - mounted disk images such as Xcode simulator runtimes (whole and
    ///     partitioned, but reported as removable).
    ///
    /// Removable physical media (USB sticks, SD cards) are excluded too, since
    /// the intent is the machine's built-in capacity. Returns 0 if IOKit can't
    /// be read.
    fn physical_disk_total() -> u64 {
        use std::ffi::{c_char, c_void};
        use std::ptr;

        type CFTypeRef = *const c_void;
        type CFStringRef = *const c_void;
        type CFAllocatorRef = *const c_void;
        type IoObject = libc::mach_port_t;

        const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
        const KCF_NUMBER_SINT64: libc::c_long = 4;

        #[link(name = "CoreFoundation", kind = "framework")]
        unsafe extern "C" {
            fn CFStringCreateWithCString(
                alloc: CFAllocatorRef,
                c_str: *const c_char,
                encoding: u32,
            ) -> CFStringRef;
            fn CFRelease(cf: CFTypeRef);
            fn CFGetTypeID(cf: CFTypeRef) -> libc::c_ulong;
            fn CFBooleanGetTypeID() -> libc::c_ulong;
            fn CFBooleanGetValue(boolean: CFTypeRef) -> u8;
            fn CFNumberGetTypeID() -> libc::c_ulong;
            fn CFNumberGetValue(
                number: CFTypeRef,
                the_type: libc::c_long,
                value: *mut c_void,
            ) -> u8;
            fn CFStringGetTypeID() -> libc::c_ulong;
            fn CFStringGetCString(
                string: CFTypeRef,
                buffer: *mut c_char,
                buffer_size: libc::c_long,
                encoding: u32,
            ) -> u8;
        }

        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            fn IOServiceMatching(name: *const c_char) -> *mut c_void;
            fn IOServiceGetMatchingServices(
                main_port: libc::mach_port_t,
                matching: *const c_void,
                existing: *mut IoObject,
            ) -> libc::kern_return_t;
            fn IOIteratorNext(iterator: IoObject) -> IoObject;
            fn IOObjectRelease(object: IoObject) -> libc::kern_return_t;
            fn IORegistryEntryCreateCFProperty(
                entry: IoObject,
                key: CFStringRef,
                allocator: CFAllocatorRef,
                options: u32,
            ) -> CFTypeRef;
        }

        // Read one named property of a registry entry as a bool / u64 / string.
        let bool_prop = |entry: IoObject, key: CFStringRef| -> bool {
            // SAFETY: `key` is a valid CFString; the returned property (if any)
            // is owned by us and released before returning.
            unsafe {
                let value = IORegistryEntryCreateCFProperty(entry, key, ptr::null(), 0);
                if value.is_null() {
                    return false;
                }
                let truthy =
                    CFGetTypeID(value) == CFBooleanGetTypeID() && CFBooleanGetValue(value) != 0;
                CFRelease(value);
                truthy
            }
        };
        let u64_prop = |entry: IoObject, key: CFStringRef| -> Option<u64> {
            // SAFETY: as above; `out` is large enough for a SInt64.
            unsafe {
                let value = IORegistryEntryCreateCFProperty(entry, key, ptr::null(), 0);
                if value.is_null() {
                    return None;
                }
                let mut out: i64 = 0;
                let ok = CFGetTypeID(value) == CFNumberGetTypeID()
                    && CFNumberGetValue(value, KCF_NUMBER_SINT64, (&mut out as *mut i64).cast())
                        != 0;
                CFRelease(value);
                ok.then_some(out as u64)
            }
        };
        let str_prop = |entry: IoObject, key: CFStringRef| -> Option<String> {
            // SAFETY: as above; the value is copied into a local buffer
            // (content strings like "GUID_partition_scheme" are short).
            unsafe {
                let value = IORegistryEntryCreateCFProperty(entry, key, ptr::null(), 0);
                if value.is_null() {
                    return None;
                }
                let mut buf = [0 as c_char; 128];
                let ok = CFGetTypeID(value) == CFStringGetTypeID()
                    && CFStringGetCString(value, buf.as_mut_ptr(), 128, KCF_STRING_ENCODING_UTF8)
                        != 0;
                CFRelease(value);
                if !ok {
                    return None;
                }
                let bytes: Vec<u8> =
                    buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
                Some(String::from_utf8_lossy(&bytes).into_owned())
            }
        };

        // SAFETY: standard IOKit enumeration. `IOServiceGetMatchingServices`
        // consumes the matching dictionary, so we never release it ourselves;
        // every iterator entry and CFString we create is released below.
        unsafe {
            let matching = IOServiceMatching(c"IOMedia".as_ptr());
            if matching.is_null() {
                return 0;
            }
            let mut iter: IoObject = 0;
            if IOServiceGetMatchingServices(0, matching, &mut iter) != 0 {
                return 0;
            }

            let key_whole =
                CFStringCreateWithCString(ptr::null(), c"Whole".as_ptr(), KCF_STRING_ENCODING_UTF8);
            let key_removable = CFStringCreateWithCString(
                ptr::null(),
                c"Removable".as_ptr(),
                KCF_STRING_ENCODING_UTF8,
            );
            let key_content = CFStringCreateWithCString(
                ptr::null(),
                c"Content".as_ptr(),
                KCF_STRING_ENCODING_UTF8,
            );
            let key_size =
                CFStringCreateWithCString(ptr::null(), c"Size".as_ptr(), KCF_STRING_ENCODING_UTF8);

            let mut total = 0u64;
            loop {
                let entry = IOIteratorNext(iter);
                if entry == 0 {
                    break;
                }
                let is_physical = bool_prop(entry, key_whole)
                    && !bool_prop(entry, key_removable)
                    && str_prop(entry, key_content)
                        .is_some_and(|c| c.ends_with("_partition_scheme"));
                if is_physical && let Some(size) = u64_prop(entry, key_size) {
                    total += size;
                }
                IOObjectRelease(entry);
            }

            CFRelease(key_whole);
            CFRelease(key_removable);
            CFRelease(key_content);
            CFRelease(key_size);
            IOObjectRelease(iter);
            total
        }
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
    // Each installed package is a subdirectory of /var/db/pkg/ — exactly what
    // `pkg_info` walks, but without spawning the tool or reading +CONTENTS.
    // Dot-prefixed entries (e.g. `.libs-*` shared-library stubs) are not
    // packages and must be skipped, matching `pkg_info`'s behavior.
    pub fn installed_package_count() -> Option<u32> {
        let entries = std::fs::read_dir("/var/db/pkg").ok()?;
        let mut n: u32 = 0;
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                n += 1;
            }
        }
        Some(n)
    }

    // kern.boottime is the Unix timestamp of last boot; subtract from now.
    pub fn uptime_seconds() -> u64 {
        let Some(boot) = sysctl("kern.boottime").and_then(|s| s.parse::<u64>().ok()) else {
            return 0;
        };
        let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
            return 0;
        };
        now.as_secs().saturating_sub(boot)
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
