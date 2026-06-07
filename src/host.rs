//! Compile-time shim around the parts of `sysinfo` that aren't implemented on
//! OpenBSD. On every other supported platform these delegate straight to
//! `sysinfo`; on OpenBSD they shell out to `uname` / `sysctl(8)` instead so the
//! about-box doesn't end up showing "Unknown" for every Software/Hardware row.
//!
//! [`window_manager`] sits outside that native/OpenBSD split: it's purely
//! environment-based (no `sysinfo`), so one definition serves every platform.

#[cfg(not(target_os = "openbsd"))]
pub use native::*;

#[cfg(target_os = "openbsd")]
pub use openbsd::*;

/// Compositor (on Wayland) or window manager (on X11) together with the
/// windowing system, for the about-box "WM" row — e.g. `"River (Wayland)"` or
/// `"i3 (X11)"`. Detection is environment-based and needs no X11/Wayland client
/// libraries, so it can't name a window manager an X11 server is hiding behind
/// EWMH; it reports what the session advertises about itself.
///
/// Returns `None` when nothing can be determined — a non-graphical (tty)
/// session, or a platform without the concept — so the row is hidden like the
/// other optional fields. macOS and Windows don't have the X11/Wayland split
/// this row describes, so the row is omitted there entirely.
pub fn window_manager() -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        match (wm_name(), windowing_system()) {
            (Some(name), Some(system)) => Some(format!("{name} ({system})")),
            (Some(name), None) => Some(name),
            // No name, but we still know we're on a graphical session.
            (None, Some(system)) => Some(format!("({system})")),
            (None, None) => None,
        }
    }
}

/// `"Wayland"` / `"X11"` for the windowing system, or `None` on a tty. A live
/// Wayland socket in the environment is the strongest signal and wins even on a
/// Wayland session running XWayland (where `DISPLAY` is set too); only with no
/// display socket at all do we fall back to the session type logind advertises.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn windowing_system() -> Option<String> {
    fn nonempty(key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    if nonempty("WAYLAND_DISPLAY").is_some() {
        return Some("Wayland".to_string());
    }
    if nonempty("DISPLAY").is_some() {
        return Some("X11".to_string());
    }
    match nonempty("XDG_SESSION_TYPE").as_deref() {
        Some("wayland") => Some("Wayland".to_string()),
        Some("x11") => Some("X11".to_string()),
        _ => None,
    }
}

/// The desktop the session advertises itself as: `XDG_CURRENT_DESKTOP`
/// ("River", "GNOME", "sway"), falling back to the older `DESKTOP_SESSION`.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn wm_name() -> Option<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .and_then(|v| normalize_desktop_name(&v))
        .or_else(|| {
            std::env::var("DESKTOP_SESSION")
                .ok()
                .and_then(|v| normalize_desktop_name(&v))
        })
}

/// Reduce a raw `XDG_CURRENT_DESKTOP` / `DESKTOP_SESSION` value to a single
/// display name. `XDG_CURRENT_DESKTOP` is a colon-separated list whose entries
/// run generic-to-specific ("ubuntu:GNOME"), so the last one is the actual
/// desktop; a leading "X-" ("X-Cinnamon") is a historical prefix and dropped.
/// Returns `None` if nothing usable remains.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn normalize_desktop_name(raw: &str) -> Option<String> {
    let last = raw.rsplit(':').next().unwrap_or(raw).trim();
    let name = last.strip_prefix("X-").unwrap_or(last).trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(all(test, not(any(target_os = "macos", target_os = "windows"))))]
mod tests {
    use super::normalize_desktop_name;

    #[test]
    fn normalize_desktop_name_picks_the_specific_entry() {
        assert_eq!(normalize_desktop_name("River").as_deref(), Some("River"));
        // Distro-prefixed lists keep the trailing, specific desktop.
        assert_eq!(
            normalize_desktop_name("ubuntu:GNOME").as_deref(),
            Some("GNOME")
        );
        assert_eq!(
            normalize_desktop_name("pop:GNOME").as_deref(),
            Some("GNOME")
        );
        // The historical "X-" prefix is dropped.
        assert_eq!(
            normalize_desktop_name("X-Cinnamon").as_deref(),
            Some("Cinnamon")
        );
        // Blank / separator-only values yield nothing.
        assert_eq!(normalize_desktop_name(""), None);
        assert_eq!(normalize_desktop_name("  "), None);
        assert_eq!(normalize_desktop_name(":"), None);
    }
}

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

    #[cfg(windows)]
    fn cpu_brand_from_registry() -> Option<String> {
        use winreg::RegKey;
        use winreg::enums::HKEY_LOCAL_MACHINE;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let cpu_key = hklm
            .open_subkey(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0")
            .ok()?;
        let name: String = cpu_key.get_value("ProcessorNameString").ok()?;
        let trimmed = name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// Read a sysctl key as a string by shelling out to `/sbin/sysctl -n`.
    /// FreeBSD's sysinfo backend leaves the CPU brand empty, so we fall back
    /// to `hw.model` just like the OpenBSD path does.
    #[cfg(target_os = "freebsd")]
    fn sysctl(key: &str) -> Option<String> {
        let out = std::process::Command::new("/sbin/sysctl")
            .args(["-n", key])
            .output()
            .ok()?;
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

    pub fn cpu_brand(sys: &System) -> Option<String> {
        // On Windows, try registry first as sysinfo may return empty brand
        #[cfg(windows)]
        if let Some(brand) = cpu_brand_from_registry() {
            return Some(brand);
        }

        // sysinfo's FreeBSD backend reports an empty brand, so read hw.model.
        #[cfg(target_os = "freebsd")]
        if let Some(brand) = sysctl("hw.model") {
            return Some(brand);
        }

        // Fall back to sysinfo for other platforms or if registry fails
        sys.cpus()
            .first()
            .map(|c| c.brand().to_string())
            .filter(|s| !s.trim().is_empty())
    }
    pub fn cpu_frequency_mhz(sys: &System) -> Option<u64> {
        // On FreeBSD the brand from `hw.model` already encodes the nominal
        // frequency (e.g. "Intel(R) Core(TM) i5-4300M CPU @ 2.60GHz"), so
        // appending sysinfo's MHz would duplicate it on the CPU row.
        #[cfg(target_os = "freebsd")]
        {
            let _ = sys;
            None
        }
        #[cfg(not(target_os = "freebsd"))]
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
        // packages varies per distro/OS, so this is filled in per platform as a
        // cheap path becomes known. Linux currently covers dpkg (Debian and
        // derivatives); other platforms stay unsupported.
        #[cfg(target_os = "linux")]
        {
            dpkg_installed_count()
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Counts the packages dpkg considers fully installed by parsing
    /// `/var/lib/dpkg/status` — the same database `dpkg-query` reads — directly,
    /// without spawning the tool. Each stanza carries a `Status:` line and only
    /// the `install ok installed` state is counted, so packages that were
    /// removed but left their config behind (`deinstall ok config-files`) are
    /// excluded. This matches
    /// `dpkg-query -f '${Status}\n' -W | grep -c 'install ok installed'`.
    /// Returns None on non-dpkg distros, where the file is absent.
    #[cfg(target_os = "linux")]
    fn dpkg_installed_count() -> Option<u32> {
        let status = std::fs::read_to_string("/var/lib/dpkg/status").ok()?;
        let n = status
            .lines()
            .filter(|line| {
                line.strip_prefix("Status:")
                    .is_some_and(|s| s.trim() == "install ok installed")
            })
            .count();
        Some(n as u32)
    }
    /// Sum of the *physical* disk capacities, read from each `\\.\PhysicalDriveN`
    /// via `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`. Removable and USB-attached drives
    /// are skipped (via `IOCTL_STORAGE_QUERY_PROPERTY`) so a plugged-in stick
    /// doesn't inflate the installed-storage figure — mirroring the OpenBSD
    /// dmesg path. Both IOCTLs work on a handle opened with zero access, so this
    /// needs no administrator rights. Returns 0 if nothing could be read.
    #[cfg(windows)]
    fn physical_disk_total_bytes() -> u64 {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::IO::DeviceIoControl;

        const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
        const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
        // STORAGE_BUS_TYPE::BusTypeUsb
        const BUS_TYPE_USB: u32 = 0x07;
        // Physical drive numbers can have gaps (e.g. after a hot-unplug), so we
        // scan a fixed range rather than stopping at the first one that's absent.
        const MAX_DRIVES: u32 = 32;

        let mut total: u64 = 0;

        for n in 0..MAX_DRIVES {
            let path = format!(r"\\.\PhysicalDrive{n}");
            let wide: Vec<u16> = OsStr::new(&path)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            // Zero desired access is enough for these metadata IOCTLs and, unlike
            // a read handle, doesn't require elevation. Sharing read+write is
            // mandatory since the OS already holds the disk open.
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                continue;
            }

            let mut returned: u32 = 0;

            // STORAGE_DEVICE_DESCRIPTOR: RemovableMedia is a byte at offset 10,
            // BusType a DWORD at offset 28. The 12-byte zeroed input is a
            // STORAGE_PROPERTY_QUERY asking for StorageDeviceProperty.
            let query = [0u8; 12];
            let mut desc = [0u8; 512];
            let got_desc = unsafe {
                DeviceIoControl(
                    handle,
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    query.as_ptr() as *const _,
                    query.len() as u32,
                    desc.as_mut_ptr() as *mut _,
                    desc.len() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            let skip = got_desc != 0 && returned >= 32 && {
                let removable = desc[10] != 0;
                let bus_type = u32::from_le_bytes([desc[28], desc[29], desc[30], desc[31]]);
                removable || bus_type == BUS_TYPE_USB
            };

            if !skip {
                // DISK_GEOMETRY_EX: the true DiskSize is an i64 at offset 24,
                // past the 24-byte DISK_GEOMETRY header.
                let mut geo = [0u8; 64];
                let got_geo = unsafe {
                    DeviceIoControl(
                        handle,
                        IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
                        std::ptr::null(),
                        0,
                        geo.as_mut_ptr() as *mut _,
                        geo.len() as u32,
                        &mut returned,
                        std::ptr::null_mut(),
                    )
                };
                if got_geo != 0 && returned >= 32 {
                    let size = i64::from_le_bytes([
                        geo[24], geo[25], geo[26], geo[27], geo[28], geo[29], geo[30], geo[31],
                    ]);
                    if size > 0 {
                        total += size as u64;
                    }
                }
            }

            unsafe { CloseHandle(handle) };
        }

        total
    }

    /// Sum of the *physical* disk capacities, read from `/sys/block/`. Each
    /// whole-disk entry's `size` file is a count of 512-byte sectors — the
    /// kernel always reports this unit regardless of the drive's real
    /// logical/physical sector size. Mirroring the OpenBSD/Windows paths, the
    /// following are excluded so they don't inflate or double-count the
    /// installed-storage figure:
    ///
    ///  - removable and USB-attached drives — a plugged-in stick or external
    ///    SSD shouldn't count as installed storage,
    ///  - virtual block devices layered over real disks: device-mapper (`dm-*`,
    ///    covering LVM and LUKS) and mdraid (`md*`), like OpenBSD's softraid,
    ///  - loop/ram/zram pseudo-devices and optical/floppy drives (`sr*`, `fd*`).
    ///
    /// `/sys/block` lists only whole disks, never partitions, so no extra
    /// partition filtering is needed. Returns 0 if it can't be read.
    #[cfg(target_os = "linux")]
    fn physical_disk_total_bytes() -> u64 {
        let Ok(entries) = std::fs::read_dir("/sys/block") else {
            return 0;
        };

        let mut total: u64 = 0;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name.starts_with("loop")
                || name.starts_with("ram")
                || name.starts_with("zram")
                || name.starts_with("dm-")
                || name.starts_with("md")
                || name.starts_with("sr")
                || name.starts_with("fd")
            {
                continue;
            }

            let base = entry.path();

            // removable == 1 covers USB sticks, SD cards and optical media.
            if std::fs::read_to_string(base.join("removable")).is_ok_and(|s| s.trim() == "1") {
                continue;
            }

            // USB-attached fixed drives report removable == 0, so additionally
            // check the bus: `/sys/block/<dev>` is a symlink into the device
            // tree, and a USB device's resolved path contains a `/usb` element.
            if std::fs::canonicalize(&base).is_ok_and(|p| p.to_string_lossy().contains("/usb")) {
                continue;
            }

            if let Some(sectors) = std::fs::read_to_string(base.join("size"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
            {
                total += sectors * 512;
            }
        }

        total
    }

    /// (total, available) for the about-box's Disk row.
    ///
    /// `available` is always the sum of free space across mounted filesystems.
    /// `total` is the installed-storage figure: on Windows the summed physical
    /// disk capacity and on Linux the summed `/sys/block` device capacity —
    /// both reporting the actual hardware size rather than just mounted
    /// volumes, and both falling back to filesystem totals if no physical disk
    /// could be read. On macOS the filesystem total already tracks the
    /// installed capacity closely enough, so it's used directly.
    pub fn disk_totals() -> (u64, u64) {
        use sysinfo::Disks;
        let mut fs_total = 0u64;
        let mut fs_avail = 0u64;
        for disk in Disks::new_with_refreshed_list().list() {
            fs_total += disk.total_space();
            fs_avail += disk.available_space();
        }

        #[cfg(any(windows, target_os = "linux"))]
        let total = {
            let physical = physical_disk_total_bytes();
            if physical > 0 { physical } else { fs_total }
        };
        #[cfg(not(any(windows, target_os = "linux")))]
        let total = fs_total;

        (total, fs_avail)
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
