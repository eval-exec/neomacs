use super::*;

#[test]
fn test_wpe_types_exist() {
    let _: *mut platform::WPEDisplay = std::ptr::null_mut();
    let _: *mut webkit::WebKitWebView = std::ptr::null_mut();
}
