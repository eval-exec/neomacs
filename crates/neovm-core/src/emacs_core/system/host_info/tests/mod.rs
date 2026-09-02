use super::*;

/// Pin GNU's precise source rather than merely checking for a plausible
/// timestamp: a kernel boot time can differ from utmp by enough to make
/// Neomacs reject a live GNU lock as stale.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn boot_time_source_is_the_utmp_boot_record_like_gnu() {
    let Some(utmp_boot) = super::native::boot_time_from_utmp() else {
        return;
    };
    assert_eq!(
        boot_time(),
        Some(utmp_boot),
        "boot time must come from the utmp BOOT_TIME record, as GNU's does"
    );
}

#[cfg(target_family = "wasm")]
#[test]
fn wasm_reports_native_inventory_as_unavailable() {
    assert_eq!(system_name(), None);
    assert_eq!(operating_system_release(), None);
    assert_eq!(load_average(), None);
    assert_eq!(configured_processor_count(), None);
    assert_eq!(boot_time(), None);
}
