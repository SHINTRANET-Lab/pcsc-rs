//! Disk activity that sysinfo does not report: the share of CPU time spent
//! waiting for IO (iowait, what Proxmox calls "IO delay") and how busy each
//! disk was (iostat's %util). On Linux both come from `/proc`. Windows has neither, so they
//! are estimated: a volume's busy time comes from the disk performance counters
//! (`IOCTL_DISK_PERFORMANCE`), and iowait is the share of time the CPU was idle while the
//! busiest disk was working. Elsewhere they are reported as unknown.

/// Keeps the previous counters so each sample can report the change since the last one.
#[derive(Default)]
pub struct Tracker {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    inner: linux::Tracker,
    #[cfg(windows)]
    inner: windows::Tracker,
}

// Elsewhere every value is unknown
#[cfg_attr(
    not(any(target_os = "linux", target_os = "android", windows)),
    allow(dead_code, clippy::unused_self)
)]
impl Tracker {
    pub fn new() -> Self {
        let mut tracker = Self::default();
        tracker.refresh();
        tracker
    }

    pub fn refresh(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "android", windows))]
        self.inner.refresh();
    }

    /// Percentage of CPU time spent waiting for IO since the previous refresh.
    pub fn iowait(&self) -> Option<f64> {
        #[cfg(any(target_os = "linux", target_os = "android", windows))]
        return self.inner.iowait;
        #[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
        None
    }

    /// Bytes read and written per second by the whole system (zram swap is not in it).
    /// Android only, where per-device statistics are not readable or not usable.
    pub fn system_io(&self) -> Option<(u64, u64)> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        return self.inner.system_io;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        None
    }

    /// Percentage of time the device (e.g. `/dev/sda1`, on Windows the mount point `C:\`)
    /// was busy since the previous refresh.
    pub fn busy(&self, device: &str) -> Option<f64> {
        #[cfg(any(target_os = "linux", target_os = "android", windows))]
        return self.inner.busy(device);
        #[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
        {
            let _ = device;
            None
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android", test))]
mod parse {
    /// CPU time counters from the first `cpu` line of `/proc/stat`.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CpuTimes {
        pub iowait: u64,
        pub total: u64,
    }

    pub fn cpu_times(stat: &str) -> Option<CpuTimes> {
        let line = stat.lines().find(|l| l.starts_with("cpu "))?;
        // user nice system idle iowait irq softirq steal (guest time is already in user)
        let fields: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .take(8)
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        (fields.len() >= 5).then(|| CpuTimes {
            iowait: fields[4],
            total: fields.iter().sum(),
        })
    }

    /// Milliseconds each device spent doing IO (`io_ticks`), by name (`sda1`, `dm-0`, …).
    pub fn io_ticks(diskstats: &str) -> Vec<(String, u64)> {
        diskstats
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace().skip(2);
                let name = fields.next()?.to_owned();
                // reads, merged, sectors, ms, writes, merged, sectors, ms, in progress, io_ticks
                let ticks = fields.nth(9)?.parse().ok()?;
                Some((name, ticks))
            })
            .collect()
    }

    /// KiB read from and written to block devices so far (`pgpgin`/`pgpgout` in
    /// `/proc/vmstat`). zram swap, which Android uses, is not counted in them.
    pub fn vm_io(vmstat: &str) -> Option<(u64, u64)> {
        let value = |key: &str| {
            vmstat
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(' '))
                .and_then(|v| v.trim().parse::<u64>().ok())
        };
        Some((value("pgpgin")?, value("pgpgout")?))
    }

    /// `part / whole` as a percentage, kept within 0–100.
    #[allow(clippy::cast_precision_loss)]
    pub fn percent(part: u64, whole: u64) -> Option<f64> {
        (whole > 0).then(|| (part as f64 / whole as f64 * 100.0).clamp(0.0, 100.0))
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux {
    use std::{collections::HashMap, fs, path::Path, time::Instant};

    use super::parse::{self, CpuTimes};

    #[derive(Default)]
    pub struct Tracker {
        cpu: Option<CpuTimes>,
        ticks: HashMap<String, u64>,
        at: Option<Instant>,
        pub iowait: Option<f64>,
        busy: HashMap<String, f64>,
        /// KiB read and written so far, from /proc/vmstat
        vm: Option<(u64, u64)>,
        pub system_io: Option<(u64, u64)>,
    }

    impl Tracker {
        pub fn refresh(&mut self) {
            let now = Instant::now();

            let cpu = fs::read_to_string("/proc/stat")
                .ok()
                .and_then(|s| parse::cpu_times(&s));
            self.iowait = match (self.cpu, cpu) {
                (Some(old), Some(new)) => parse::percent(
                    new.iowait.saturating_sub(old.iowait),
                    new.total.saturating_sub(old.total),
                ),
                _ => None,
            };
            self.cpu = cpu;

            let diskstats = fs::read_to_string("/proc/diskstats").ok();
            // Android keeps /proc/diskstats (and /sys/block/*/stat) from everyone but
            // root; fall back to the system-wide totals in /proc/vmstat. Where it is readable
            // (su), sysinfo still cannot match /dev/block/dm-N to its diskstats line, so
            // Android always uses the totals.
            self.system_io = None;
            if diskstats.is_none() || crate::android::is_android() {
                let vm = fs::read_to_string("/proc/vmstat")
                    .ok()
                    .and_then(|s| parse::vm_io(&s));
                if let (Some(at), Some((old_r, old_w)), Some((r, w))) = (self.at, self.vm, vm) {
                    let millis =
                        u64::try_from(now.duration_since(at).as_millis()).unwrap_or(u64::MAX);
                    let rate = |kib: u64| (kib * 1024 * 1000).checked_div(millis).unwrap_or(0);
                    self.system_io =
                        Some((rate(r.saturating_sub(old_r)), rate(w.saturating_sub(old_w))));
                }
                self.vm = vm;
            }
            let ticks: HashMap<String, u64> = diskstats
                .map(|s| parse::io_ticks(&s).into_iter().collect())
                .unwrap_or_default();
            self.busy.clear();
            if let Some(at) = self.at {
                let elapsed = u64::try_from(now.duration_since(at).as_millis()).unwrap_or(u64::MAX);
                for (name, &new) in &ticks {
                    if let Some(&old) = self.ticks.get(name)
                        && let Some(busy) = parse::percent(new.saturating_sub(old), elapsed)
                    {
                        self.busy.insert(name.clone(), busy);
                    }
                }
            }
            self.ticks = ticks;
            self.at = Some(now);
        }

        /// `/dev/sda1` → `sda1`, `/dev/mapper/vg-root` → `dm-0` (the name in `/proc/diskstats`).
        pub fn busy(&self, device: &str) -> Option<f64> {
            if !device.starts_with("/dev/") {
                return None;
            }
            let real = fs::canonicalize(device).ok()?;
            let name = Path::new(&real).file_name()?.to_str()?;
            self.busy.get(name).copied()
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::{collections::HashMap, ptr};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, GetLogicalDrives,
            OPEN_EXISTING,
        },
        System::{
            IO::DeviceIoControl,
            Ioctl::{DISK_PERFORMANCE, IOCTL_DISK_PERFORMANCE},
            Threading::GetSystemTimes,
        },
    };

    /// `DRIVE_FIXED`: hard disks and SSDs (not CD drives, which would spin up)
    const DRIVE_FIXED: u32 = 3;

    #[derive(Default)]
    pub struct Tracker {
        /// Idle and total CPU time at the previous refresh
        cpu: Option<(u64, u64)>,
        /// Query time and idle time (100 ns units) of each volume at the previous refresh
        disks: HashMap<char, (i64, i64)>,
        pub iowait: Option<f64>,
        busy: HashMap<char, f64>,
    }

    impl Tracker {
        #[allow(clippy::cast_precision_loss)]
        pub fn refresh(&mut self) {
            // The CPU's idle share since the previous refresh
            let cpu = system_times();
            let idle_share = match (self.cpu, cpu) {
                (Some((old_idle, old_total)), Some((idle, total))) => {
                    let total = total.saturating_sub(old_total);
                    (total > 0).then(|| {
                        (idle.saturating_sub(old_idle) as f64 / total as f64 * 100.0)
                            .clamp(0.0, 100.0)
                    })
                }
                _ => None,
            };
            self.cpu = cpu;

            // Each volume's busy share: what the time without IO in flight leaves
            let mut disks = HashMap::new();
            self.busy.clear();
            for letter in fixed_drives() {
                let Some((query, idle)) = disk_times(letter) else {
                    continue;
                };
                if let Some(&(old_query, old_idle)) = self.disks.get(&letter) {
                    let elapsed = query - old_query;
                    if elapsed > 0 {
                        let idle_share = (idle - old_idle) as f64 / elapsed as f64 * 100.0;
                        self.busy
                            .insert(letter, (100.0 - idle_share).clamp(0.0, 100.0));
                    }
                }
                disks.insert(letter, (query, idle));
            }
            self.disks = disks;

            // CPU idle while the busiest disk was working
            let busiest = self.busy.values().copied().fold(0.0, f64::max);
            self.iowait = idle_share.map(|idle| idle * busiest / 100.0);
        }

        /// `C:\` → the volume `C:`
        pub fn busy(&self, mount_point: &str) -> Option<f64> {
            let letter = mount_point.chars().next()?.to_ascii_uppercase();
            self.busy.get(&letter).copied()
        }
    }

    fn file_time(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }

    /// Idle and total (kernel, which includes idle, plus user) CPU time, 100 ns units
    fn system_times() -> Option<(u64, u64)> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut idle, mut kernel, mut user) = (zero, zero, zero);
        // SAFETY: three valid FILETIMEs to write to
        let ok = unsafe { GetSystemTimes(&raw mut idle, &raw mut kernel, &raw mut user) };
        (ok != 0).then(|| (file_time(idle), file_time(kernel) + file_time(user)))
    }

    fn fixed_drives() -> Vec<char> {
        // SAFETY: no arguments
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|bit| mask >> bit & 1 == 1)
            .map(|bit| char::from(b'A' + bit))
            .filter(|letter| {
                let root: Vec<u16> = format!("{letter}:\\")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                // SAFETY: a null-terminated wide string
                unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
            })
            .collect()
    }

    /// `QueryTime` and `IdleTime` of the volume (`\\.\C:`), 100 ns units. Opened without any
    /// access, which is enough to ask for the counters; `None` where it is refused.
    fn disk_times(letter: char) -> Option<(i64, i64)> {
        let path: Vec<u16> = format!(r"\\.\{letter}:")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: valid arguments; the handle is closed before returning
        unsafe {
            let handle = CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut perf: DISK_PERFORMANCE = std::mem::zeroed();
            let mut returned = 0u32;
            let ok = DeviceIoControl(
                handle,
                IOCTL_DISK_PERFORMANCE,
                ptr::null(),
                0,
                (&raw mut perf).cast(),
                u32::try_from(size_of::<DISK_PERFORMANCE>()).unwrap_or(0),
                &raw mut returned,
                ptr::null_mut(),
            );
            CloseHandle(handle);
            (ok != 0).then_some((perf.QueryTime, perf.IdleTime))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse::{CpuTimes, cpu_times, io_ticks, percent};

    #[test]
    fn reads_cpu_times() {
        let stat = "cpu  100 5 50 800 40 3 2 0 7 0\ncpu0 50 2 25 400 20 1 1 0 3 0\nintr 1 2 3\n";
        assert_eq!(
            cpu_times(stat),
            Some(CpuTimes {
                iowait: 40,
                total: 1000
            })
        );
        assert_eq!(cpu_times("intr 1 2 3\n"), None);
    }

    #[test]
    fn reads_io_ticks() {
        let diskstats = "\
   8       0 sda 1000 10 20000 300 500 5 8000 200 0 450 600 0 0 0 0 0 0
   8       1 sda1 900 10 18000 280 480 5 7600 190 0 420 570 0 0 0 0 0 0
 253       0 dm-0 50 0 400 10 20 0 160 5 0 12 15
   7       0 loop0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
";
        assert_eq!(
            io_ticks(diskstats),
            [
                ("sda".to_owned(), 450),
                ("sda1".to_owned(), 420),
                ("dm-0".to_owned(), 12),
                ("loop0".to_owned(), 0)
            ]
        );
        assert_eq!(io_ticks("8 0 short 1 2 3\n"), []);
    }

    #[test]
    fn reads_vm_io() {
        use super::parse::vm_io;
        let vmstat =
            "nr_free_pages 1\npgpgin 74489752\npgpgout 9733068\npswpin 1000\npswpout 2000\n";
        assert_eq!(vm_io(vmstat), Some((74_489_752, 9_733_068)));
        assert_eq!(vm_io("pgpgin 10\npgpgout 20\n"), Some((10, 20)));
        assert_eq!(vm_io("nr_free_pages 1\n"), None);
    }

    #[test]
    fn percentages() {
        assert_eq!(percent(40, 1000), Some(4.0));
        assert_eq!(percent(1500, 1000), Some(100.0));
        assert_eq!(percent(1, 0), None);
    }
}
