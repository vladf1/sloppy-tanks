//! Process and host figures for the monitor: CPU time, memory, load and allocation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

/// The system allocator plus a count of live bytes, which the dashboard shows as heap
/// used (and its high-water mark as heap total). The binary installs it; test builds
/// leave it out and report zero.
pub struct CountingAllocator;

// SAFETY: every call forwards to `System` with the caller's layout and pointer unchanged;
// the counters only observe sizes.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            grow(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            grow(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            if new_size >= layout.size() {
                grow(new_size - layout.size());
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        moved
    }
}

fn grow(bytes: usize) {
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

/// Bytes currently allocated through [`CountingAllocator`].
pub fn heap_used_bytes() -> usize {
    LIVE_BYTES.load(Ordering::Relaxed)
}

/// Most bytes ever allocated at once through [`CountingAllocator`].
pub fn heap_peak_bytes() -> usize {
    PEAK_BYTES.load(Ordering::Relaxed)
}

/// User plus system CPU time of the whole process (every thread), in microseconds.
pub fn cpu_micros() -> u64 {
    // SAFETY: getrusage fills the zeroed struct it is given.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let micros = |time: libc::timeval| time.tv_sec as u64 * 1_000_000 + time.tv_usec as u64;
    micros(usage.ru_utime) + micros(usage.ru_stime)
}

/// Resident set size in bytes.
#[cfg(target_os = "linux")]
pub fn rss_bytes() -> u64 {
    let pages = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|text| text.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0);
    pages * page_size()
}

/// Resident set size in bytes.
#[cfg(target_os = "macos")]
pub fn rss_bytes() -> u64 {
    // SAFETY: proc_pidinfo writes at most the given size into the zeroed struct.
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let written = libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTASKINFO,
            0,
            (&raw mut info).cast(),
            size,
        );
        if written == size {
            info.pti_resident_size
        } else {
            0
        }
    }
}

#[cfg(target_os = "linux")]
fn page_size() -> u64 {
    // SAFETY: sysconf has no preconditions.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 { size as u64 } else { 4096 }
}

/// The host's one-minute load average.
pub fn load_average() -> f64 {
    let mut loads = [0f64; 3];
    // SAFETY: getloadavg writes at most the given number of samples.
    let written = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if written >= 1 { loads[0] } else { 0.0 }
}

pub fn cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Total and available host memory in bytes (MemAvailable on Linux).
#[cfg(target_os = "linux")]
pub fn host_memory() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .map_or(0, |kib| kib * 1024)
    };
    (field("MemTotal:"), field("MemAvailable:"))
}

/// Total and free host memory in bytes.
#[cfg(target_os = "macos")]
pub fn host_memory() -> (u64, u64) {
    fn sysctl<T: Default + Copy>(name: &std::ffi::CStr) -> T {
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>();
        // SAFETY: sysctlbyname writes at most `size` bytes into `value`.
        let status = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                (&raw mut value).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if status == 0 { value } else { T::default() }
    }
    let total: u64 = sysctl(c"hw.memsize");
    let free_pages: u32 = sysctl(c"vm.page_free_count");
    let page: u64 = sysctl::<libc::c_int>(c"hw.pagesize") as u64;
    (total, u64::from(free_pages) * page)
}

/// Where the process runs, for the dashboard, such as "Docker container · Linux 7.0.0".
/// Docker and Podman put a marker file at a container's root. A container shares its
/// host's kernel, while the server image holds no OS files, so only a host shows its
/// distribution.
pub fn environment() -> String {
    let container = if std::path::Path::new("/.dockerenv").exists() {
        Some("Docker container")
    } else if std::path::Path::new("/run/.containerenv").exists() {
        Some("Podman container")
    } else {
        None
    };
    let read = |path| std::fs::read_to_string(path).ok();
    describe_environment(
        container,
        read("/etc/os-release").as_deref(),
        read("/proc/sys/kernel/osrelease").as_deref(),
    )
}

fn describe_environment(
    container: Option<&str>,
    os_release: Option<&str>,
    kernel_release: Option<&str>,
) -> String {
    let mut parts = vec![container.unwrap_or("no container").to_string()];
    let distribution = os_release
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        })
        .map(|name| name.trim_matches('"').to_string());
    parts.extend(distribution);
    parts.push(match kernel_release {
        Some(release) => format!("Linux {}", release.trim()),
        None => std::env::consts::OS.to_string(),
    });
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_a_container_by_its_marker_and_kernel() {
        assert_eq!(
            describe_environment(Some("Docker container"), None, Some("7.0.0-14-generic\n")),
            "Docker container · Linux 7.0.0-14-generic"
        );
    }

    #[test]
    fn describes_a_host_with_its_distribution() {
        let os_release = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 26.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(
            describe_environment(None, Some(os_release), Some("7.0.0-14-generic\n")),
            "no container · Ubuntu 26.04.1 LTS · Linux 7.0.0-14-generic"
        );
        assert_eq!(
            describe_environment(None, None, None),
            format!("no container · {}", std::env::consts::OS)
        );
    }

    #[test]
    fn process_figures_are_plausible() {
        let before = cpu_micros();
        assert!(cpu_micros() >= before);
        assert!(rss_bytes() > 1024 * 1024);
        let (total, available) = host_memory();
        assert!(total > 0 && available <= total);
        assert!(load_average() >= 0.0);
        assert!(cpu_count() >= 1);
    }
}
