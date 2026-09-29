//! Process and host figures for the monitor: CPU time, memory, load and allocation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

/// The system allocator plus a count of live bytes, which stands in for Node's
/// `heapUsed` (and its high-water mark for `heapTotal`). The binary installs it; test
/// builds leave it out and report zero.
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

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn rss_bytes() -> u64 {
    0
}

#[cfg(target_os = "linux")]
fn page_size() -> u64 {
    // SAFETY: sysconf has no preconditions.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 { size as u64 } else { 4096 }
}

/// The host's one-minute load average (`os.loadavg()[0]`).
pub fn load_average() -> f64 {
    let mut loads = [0f64; 3];
    // SAFETY: getloadavg writes at most the given number of samples.
    let written = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if written >= 1 { loads[0] } else { 0.0 }
}

pub fn cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Total and available host memory in bytes (`os.totalmem()` / `os.freemem()`, which
/// libuv reads from MemAvailable on Linux).
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

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn host_memory() -> (u64, u64) {
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_figures_are_plausible() {
        let before = cpu_micros();
        let mut spin = 0u64;
        for index in 0..2_000_000u64 {
            spin = spin.wrapping_add(index * index);
        }
        std::hint::black_box(spin);
        assert!(cpu_micros() >= before);
        assert!(rss_bytes() > 1024 * 1024);
        let (total, available) = host_memory();
        assert!(total > 0 && available <= total);
        assert!(load_average() >= 0.0);
        assert!(cpu_count() >= 1);
    }
}
