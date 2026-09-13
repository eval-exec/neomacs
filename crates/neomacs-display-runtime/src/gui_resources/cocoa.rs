use objc2::{
    msg_send,
    rc::{Retained, autoreleasepool},
};
use objc2_foundation::{NSString, NSUserDefaults};

pub(super) fn set(key: &str, value: Option<&str>) {
    autoreleasepool(|_| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str(key);
        if let Some(value) = value {
            let value = NSString::from_str(value);
            // SAFETY: NSString is a supported UserDefaults property-list value.
            unsafe {
                defaults.setObject_forKey(Some(&value), &key);
            }
        } else {
            defaults.removeObjectForKey(&key);
        }
    });
}

pub(super) fn get(key: &str) -> Option<String> {
    autoreleasepool(|_| {
        let defaults = NSUserDefaults::standardUserDefaults();
        let object = defaults.objectForKey(&NSString::from_str(key))?;
        // GNU formats the object with %@ rather than requiring a string.
        // SAFETY: UserDefaults contains property-list objects, all of which
        // implement NSObject's description returning an autoreleased NSString.
        let description: Retained<NSString> = unsafe { msg_send![&*object, description] };
        Some(description.to_string())
    })
}
