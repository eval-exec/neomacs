use super::*;
#[test]
#[cfg(target_pointer_width = "64")]
fn extension_abi_matches_vulkan_headers() {
    assert_eq!(std::mem::size_of::<TimingFeatures>(), 32);
    assert_eq!(std::mem::size_of::<PastTiming>(), 72);
    assert_eq!(std::mem::size_of::<PastProperties>(), 48);
    assert_eq!(std::mem::offset_of!(PastTiming, stages), 40);
    assert_eq!(std::mem::offset_of!(PastTiming, domain_id), 56);
    assert_eq!(std::mem::size_of::<SwapchainTimestamp>(), 40);
}
