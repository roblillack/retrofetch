//! Total and available space of the machine's local storage.
//!
//! We deliberately don't use `sysinfo` for this. On macOS a single APFS
//! container is surfaced as several browsable volumes (`/`,
//! `/System/Volumes/Data`, ...), and *each* reports the whole container's
//! capacity and shared free pool. Summing what `sysinfo` returns therefore
//! double- (or triple-) counts the disk.
//!
//! - **Total** is the *physical* capacity of the machine's drives. On macOS we
//!   read it straight from IOKit (the size of every whole, non-virtual
//!   `IOMedia`), which is what Disk Utility/`diskutil` report and ignores APFS
//!   containers, partitions and disk images. Elsewhere we sum the distinct
//!   mounted filesystems.
//! - **Available** is the free space a user can actually fill, taken from the
//!   live filesystem(s) — `getmntinfo`/`statvfs` on Unix,
//!   `GetDiskFreeSpaceExW` on Windows. The per-mount backing device
//!   (`f_mntfromname`) is what lets us collapse the volumes of one APFS
//!   container into a single entry while still summing independent filesystems.

use std::collections::HashSet;

/// Aggregate capacity of the machine's local, writable filesystems.
pub struct DiskSpace {
    pub total: u64,
    pub available: u64,
}

/// A single mounted filesystem worth counting. Collectors are responsible for
/// filtering out pseudo, read-only and network mounts before constructing one;
/// [`aggregate`] only deduplicates and sums.
struct Mount {
    /// Backing device, e.g. `/dev/disk3s5` (macOS), `/dev/sda1` (Linux),
    /// `C:\` (Windows).
    device: String,
    file_system: String,
    total: u64,
    available: u64,
}

/// Collapse mounts that share a free-space pool, then sum the rest.
///
/// Returns `None` when nothing countable was found (total of zero), which the
/// caller renders as "unavailable".
fn aggregate(mounts: &[Mount]) -> Option<DiskSpace> {
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let mut available = 0u64;
    for m in mounts {
        if seen.insert(dedup_key(&m.device, &m.file_system)) {
            total += m.total;
            available += m.available;
        }
    }
    (total > 0).then_some(DiskSpace { total, available })
}

/// Key identifying the shared free-space pool a mount draws from.
///
/// APFS volumes within one container all report the container's capacity, so
/// they collapse to the container (`/dev/disk3`). Every other filesystem owns
/// its space independently and is keyed by its own device.
fn dedup_key(device: &str, file_system: &str) -> String {
    if file_system.eq_ignore_ascii_case("apfs")
        && let Some(container) = apfs_container(device)
    {
        return container.to_string();
    }
    device.to_string()
}

/// `/dev/disk3s1s1` and `/dev/disk3s5` → `/dev/disk3`; non-APFS paths → `None`.
fn apfs_container(device: &str) -> Option<&str> {
    const PREFIX: &str = "/dev/disk";
    let after = device.strip_prefix(PREFIX)?;
    let digits = after.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        None
    } else {
        Some(&device[..PREFIX.len() + digits])
    }
}

/// Total/available space of the local storage, or `None` if it can't be read.
///
/// On macOS `total` is the physical drive capacity (from IOKit) while
/// `available` is the free space of the live filesystem.
#[cfg(target_os = "macos")]
pub fn disk_space() -> Option<DiskSpace> {
    let volume = aggregate(&collect_mounts());
    match physical_disk_total() {
        // IOKit gave us the physical capacity; pair it with the real free space.
        total if total > 0 => Some(DiskSpace {
            total,
            available: volume.map_or(0, |v| v.available),
        }),
        // IOKit unavailable — fall back to the filesystem's own figures.
        _ => volume,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn disk_space() -> Option<DiskSpace> {
    aggregate(&collect_mounts())
}

/// Sum the capacity of every physical drive via IOKit.
///
/// A physical drive is a *whole* `IOMedia` that is non-removable and carries a
/// partition scheme as its content (`*_partition_scheme`). That selects the
/// SSD/HDD itself while excluding:
///   - partitions (not whole);
///   - APFS / CoreStorage containers (whole, but their content is a synthesized
///     container GUID, not a partition scheme);
///   - mounted disk images such as Xcode simulator runtimes (whole and
///     partitioned, but reported as removable).
///
/// Removable physical media (USB sticks, SD cards) are excluded too, since the
/// intent is the machine's built-in capacity. Returns 0 if IOKit can't be read.
#[cfg(target_os = "macos")]
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
        fn CFNumberGetValue(number: CFTypeRef, the_type: libc::c_long, value: *mut c_void) -> u8;
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

    // Read one named property of a registry entry as a bool / u64.
    let bool_prop = |entry: IoObject, key: CFStringRef| -> bool {
        // SAFETY: `key` is a valid CFString; the returned property (if any) is
        // owned by us and released before returning.
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
                && CFNumberGetValue(value, KCF_NUMBER_SINT64, (&mut out as *mut i64).cast()) != 0;
            CFRelease(value);
            ok.then_some(out as u64)
        }
    };
    let str_prop = |entry: IoObject, key: CFStringRef| -> Option<String> {
        // SAFETY: as above; the value is copied into a local buffer (content
        // strings like "GUID_partition_scheme" are short) before release.
        unsafe {
            let value = IORegistryEntryCreateCFProperty(entry, key, ptr::null(), 0);
            if value.is_null() {
                return None;
            }
            let mut buf = [0 as c_char; 128];
            let ok = CFGetTypeID(value) == CFStringGetTypeID()
                && CFStringGetCString(value, buf.as_mut_ptr(), 128, KCF_STRING_ENCODING_UTF8) != 0;
            CFRelease(value);
            if !ok {
                return None;
            }
            let bytes: Vec<u8> = buf
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect();
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
    };

    // SAFETY: standard IOKit enumeration. `IOServiceGetMatchingServices`
    // consumes the matching dictionary, so we never release it ourselves; every
    // iterator entry and CFString we create is released below.
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
        let key_removable =
            CFStringCreateWithCString(ptr::null(), c"Removable".as_ptr(), KCF_STRING_ENCODING_UTF8);
        let key_content =
            CFStringCreateWithCString(ptr::null(), c"Content".as_ptr(), KCF_STRING_ENCODING_UTF8);
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
                && str_prop(entry, key_content).is_some_and(|c| c.ends_with("_partition_scheme"));
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

// ---------------------------------------------------------------------------
// macOS / iOS: enumerate mounts via `getmntinfo`, dedupe by APFS container.
// ---------------------------------------------------------------------------
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn collect_mounts() -> Vec<Mount> {
    let mut buf: *mut libc::statfs = std::ptr::null_mut();
    // SAFETY: `getmntinfo` fills `buf` with a pointer to a statically owned
    // array of `count` entries; we never free it and copy out before any
    // subsequent call could overwrite it.
    let count = unsafe { libc::getmntinfo(&mut buf, libc::MNT_NOWAIT) };
    if count <= 0 || buf.is_null() {
        return Vec::new();
    }
    let entries = unsafe { std::slice::from_raw_parts(buf, count as usize) };

    let mut mounts = Vec::new();
    for e in entries {
        let flags = e.f_flags;
        // Keep only browsable, local volumes — the same set Finder and Disk
        // Utility show. `MNT_DONTBROWSE` hides APFS snapshots, the auxiliary
        // system containers (Preboot/VM/Recovery/Update and the iBoot
        // container) and mounted disk images such as Xcode simulator runtimes.
        // On a modern Mac this leaves exactly one browsable volume per real
        // APFS container — the read-only system volume `/`, which still
        // reports its container's full capacity and shared free space.
        if flags & libc::MNT_DONTBROWSE as u32 != 0 || flags & libc::MNT_LOCAL as u32 == 0 {
            continue;
        }
        let device = c_str_array(&e.f_mntfromname);
        // Real volumes are `/dev/...`; skip devfs, autofs maps and the like.
        if !device.starts_with("/dev/") {
            continue;
        }
        let block = e.f_bsize as u64;
        mounts.push(Mount {
            device,
            file_system: c_str_array(&e.f_fstypename),
            total: e.f_blocks * block,
            available: e.f_bavail * block,
        });
    }
    mounts
}

/// Decode a NUL-terminated `c_char` array (e.g. `statfs.f_mntfromname`).
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn c_str_array(buf: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// Linux / Android: parse `/proc/mounts`, size each via `statvfs`.
// ---------------------------------------------------------------------------
#[cfg(any(target_os = "linux", target_os = "android"))]
fn collect_mounts() -> Vec<Mount> {
    let table = std::fs::read_to_string("/proc/mounts")
        .or_else(|_| std::fs::read_to_string("/etc/mtab"))
        .unwrap_or_default();

    let mut mounts = Vec::new();
    for line in table.lines() {
        let mut fields = line.split(' ');
        let (device, mount_point, file_system, options) =
            match (fields.next(), fields.next(), fields.next(), fields.next()) {
                (Some(d), Some(m), Some(f), Some(o)) => (d, m, f, o),
                _ => continue,
            };
        // Only real block devices. This drops tmpfs/proc/sysfs/cgroup/overlay
        // (named e.g. `tmpfs`, not `/dev/...`).
        if !device.starts_with("/dev/") {
            continue;
        }
        // Skip read-only mounts, which also drops snap squashfs loops.
        if options.split(',').any(|o| o == "ro") {
            continue;
        }
        if let Some((total, available)) = statvfs_space(&unescape_octal(mount_point)) {
            mounts.push(Mount {
                device: device.to_string(),
                file_system: file_system.to_string(),
                total,
                available,
            });
        }
    }
    mounts
}

/// Undo the octal escapes (`\040` space, `\011` tab, ...) used in `/proc/mounts`.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn unescape_octal(path: &str) -> String {
    if !path.contains('\\') {
        return path.to_string();
    }
    let bytes = path.as_bytes();
    let mut out = String::with_capacity(path.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let octal = &path[i + 1..i + 4];
            if let Ok(code) = u8::from_str_radix(octal, 8) {
                out.push(code as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Other Unix (BSDs, etc.): no portable mount enumerator wired up, so report
// the root filesystem. statvfs is POSIX and available everywhere.
// ---------------------------------------------------------------------------
#[cfg(all(
    unix,
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    ))
))]
fn collect_mounts() -> Vec<Mount> {
    match statvfs_space("/") {
        Some((total, available)) => vec![Mount {
            device: "/".to_string(),
            file_system: String::new(),
            total,
            available,
        }],
        None => Vec::new(),
    }
}

/// Total/available bytes for the filesystem backing `path`, via `statvfs(2)`.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
// The `statvfs` block-count fields are `c_ulong`/`fsblkcnt_t`, whose width
// varies by platform (u64 on 64-bit, u32 on 32-bit). The casts are needed on
// the narrow platforms but redundant on 64-bit, so silence the lint there.
#[allow(clippy::unnecessary_cast)]
fn statvfs_space(path: &str) -> Option<(u64, u64)> {
    let c_path = std::ffi::CString::new(path).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated string; `statvfs` fully
    // initialises `stat` on success (return value 0).
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    let block = if stat.f_frsize != 0 {
        stat.f_frsize
    } else {
        stat.f_bsize
    } as u64;
    Some((stat.f_blocks as u64 * block, stat.f_bavail as u64 * block))
}

// ---------------------------------------------------------------------------
// Windows: sum the fixed (non-removable) logical drives.
// ---------------------------------------------------------------------------
#[cfg(windows)]
fn collect_mounts() -> Vec<Mount> {
    const DRIVE_FIXED: u32 = 3;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }

    // SAFETY: bitmask of present drive letters, A=bit0 .. Z=bit25.
    let mask = unsafe { GetLogicalDrives() };
    let mut mounts = Vec::new();
    for i in 0..26u32 {
        if mask & (1u32 << i) == 0 {
            continue;
        }
        let letter = b'A' + i as u8;
        // Root path "X:\" as a NUL-terminated UTF-16 string.
        let root: [u16; 4] = [u16::from(letter), u16::from(b':'), u16::from(b'\\'), 0];
        // SAFETY: `root` is a valid NUL-terminated wide string.
        if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
            continue;
        }
        let mut free_to_caller = 0u64;
        let mut total = 0u64;
        let mut total_free = 0u64;
        // SAFETY: all three out-pointers are valid for the duration of the call.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                root.as_ptr(),
                &mut free_to_caller,
                &mut total,
                &mut total_free,
            )
        };
        if ok != 0 && total > 0 {
            mounts.push(Mount {
                device: format!("{}:\\", letter as char),
                file_system: String::new(),
                total,
                available: total_free,
            });
        }
    }
    mounts
}

// Fallback for any target that is neither Unix nor Windows.
#[cfg(not(any(unix, windows)))]
fn collect_mounts() -> Vec<Mount> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(device: &str, file_system: &str, total: u64, available: u64) -> Mount {
        Mount {
            device: device.to_string(),
            file_system: file_system.to_string(),
            total,
            available,
        }
    }

    #[test]
    fn apfs_container_is_parsed() {
        assert_eq!(apfs_container("/dev/disk3s1s1"), Some("/dev/disk3"));
        assert_eq!(apfs_container("/dev/disk3s5"), Some("/dev/disk3"));
        assert_eq!(apfs_container("/dev/disk12s1"), Some("/dev/disk12"));
        assert_eq!(apfs_container("/dev/sda1"), None);
        assert_eq!(apfs_container("/dev/disk"), None);
    }

    #[test]
    fn apfs_volume_group_counted_once() {
        // The two halves of one APFS volume group each report the whole
        // container — the exact macOS double-counting bug.
        let mounts = vec![
            mount("/dev/disk3s1s1", "apfs", 994_662_584_320, 56_631_625_758),
            mount("/dev/disk3s5", "apfs", 994_662_584_320, 56_631_625_758),
        ];
        let space = aggregate(&mounts).unwrap();
        assert_eq!(space.total, 994_662_584_320);
        assert_eq!(space.available, 56_631_625_758);
    }

    #[test]
    fn independent_devices_are_summed() {
        let mounts = vec![
            mount("/dev/sda1", "ext4", 100, 40),
            mount("/dev/sdb1", "ext4", 200, 50),
        ];
        let space = aggregate(&mounts).unwrap();
        assert_eq!(space.total, 300);
        assert_eq!(space.available, 90);
    }

    #[test]
    fn non_apfs_partitions_on_one_disk_are_not_merged() {
        // HFS+ partitions own their space independently, so they must sum even
        // though they live on the same physical disk.
        let mounts = vec![
            mount("/dev/disk4s1", "hfs", 100, 10),
            mount("/dev/disk4s2", "hfs", 100, 20),
        ];
        assert_eq!(aggregate(&mounts).unwrap().total, 200);
    }

    #[test]
    fn nothing_countable_is_none() {
        assert!(aggregate(&[]).is_none());
        assert!(aggregate(&[mount("/dev/sda1", "ext4", 0, 0)]).is_none());
    }
}
