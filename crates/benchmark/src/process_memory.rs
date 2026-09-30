//! Platform high-water memory counters for benchmark evidence.

/// Returns the process lifetime peak resident set/working-set size in bytes.
///
/// This is deliberately a high-water mark rather than a before/after delta:
/// the corpus benchmark reports the complete runner footprint, including
/// startup, corpus loading, evaluation, and report construction.
#[cfg(windows)]
pub(crate) fn peak_resident_set_bytes() -> Option<u64> {
    use std::ffi::c_void;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };

    // SAFETY: both functions are process-local Windows APIs. `counters` has
    // the documented C layout and remains valid for the duration of the call.
    let succeeded = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    (succeeded != 0 && counters.peak_working_set_size > 0)
        .then_some(counters.peak_working_set_size as u64)
}

#[cfg(unix)]
pub(crate) fn peak_resident_set_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `getrusage` initializes the supplied `rusage` when it returns
    // zero. The value is not read on failure.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the successful call above initialized every field of `usage`.
    let usage = unsafe { usage.assume_init() };
    let high_water = u64::try_from(usage.ru_maxrss).ok()?;
    if high_water == 0 {
        return None;
    }

    // Darwin reports bytes; Linux and the BSD-style interfaces report KiB.
    #[cfg(target_vendor = "apple")]
    return Some(high_water);
    #[cfg(not(target_vendor = "apple"))]
    return high_water.checked_mul(1024);
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn peak_resident_set_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_native_platform_reports_nonzero_peak_memory() {
        let peak = peak_resident_set_bytes();
        #[cfg(any(windows, unix))]
        assert!(peak.is_some_and(|bytes| bytes > 0));
        #[cfg(not(any(windows, unix)))]
        assert_eq!(peak, None);
    }
}
