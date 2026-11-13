//! Scalar pointer-ancestry regression shapes, not a Context/runtime/JIT model.
//! `baseline-ancestor` deliberately selects the old failing pointer ancestry.

#[cfg(all(feature = "baseline-ancestor", not(miri)))]
compile_error!("baseline-ancestor is an archived invalid pointer model; run it only under Miri");

unsafe fn callback(raw: *mut u64) {
    let borrowed = unsafe { &mut *raw };
    *borrowed += 10;
}

fn release(owner: &mut u64) {
    *owner -= 1;
}

// Baseline expansion forwards precisely the older raw pointer, without any
// reference access or helper-argument borrow that could activate owner.
macro_rules! callback_pointer {
    ($old:ident, $owner:ident) => {{
        #[cfg(feature = "baseline-ancestor")]
        {
            $old
        }
        #[cfg(not(feature = "baseline-ancestor"))]
        {
            std::ptr::from_mut($owner)
        }
    }};
}

unsafe fn finish_shape(raw: *mut u64) {
    let owner = unsafe { &mut *raw };
    // No owner access precedes the first callback, just as call_spec_finish.
    unsafe { callback(callback_pointer!(raw, owner)) };
    release(owner);
    unsafe { callback(callback_pointer!(raw, owner)) };
    *owner -= 1;
}

// The three entry alternatives are numbered scalar callbacks, not fake
// native ABIs. The relevant production operation before them is the real
// typed-owner depth increment; the owner is used afterward for frame pops.
macro_rules! macro_shape {
    ($raw:ident, $arm:expr) => {{
        let owner = unsafe { &mut *$raw };
        *owner += 1;
        match $arm {
            0 => unsafe { callback(callback_pointer!($raw, owner)) },
            1 => unsafe { callback(callback_pointer!($raw, owner)) },
            2 => unsafe { callback(callback_pointer!($raw, owner)) },
            _ => unreachable!(),
        }
        release(owner);
    }};
}

unsafe fn source_shape(raw: *mut u64, arm: u8) {
    let owner = unsafe { &mut *raw };
    *owner += 1;
    if arm == 0 {
        unsafe { callback(callback_pointer!(raw, owner)) };
    } else if arm == 1 {
        unsafe { callback(callback_pointer!(raw, owner)) };
    } else {
        unsafe { callback(callback_pointer!(raw, owner)) };
    }
    release(owner);
}

#[test]
fn finish_callbacks_preserve_owner() {
    let mut counter = 1;
    unsafe { finish_shape(&mut counter) };
    assert_eq!(counter, 19);
}

#[test]
fn macro_arm_0_preserves_owner() {
    let mut counter = 1;
    let raw = std::ptr::from_mut(&mut counter);
    macro_shape!(raw, 0);
    assert_eq!(counter, 11);
}

#[test]
fn macro_arm_1_preserves_owner() {
    let mut counter = 1;
    let raw = std::ptr::from_mut(&mut counter);
    macro_shape!(raw, 1);
    assert_eq!(counter, 11);
}

#[test]
fn macro_arm_2_preserves_owner() {
    let mut counter = 1;
    let raw = std::ptr::from_mut(&mut counter);
    macro_shape!(raw, 2);
    assert_eq!(counter, 11);
}

#[test]
fn source_arm_0_preserves_owner() {
    let mut counter = 1;
    unsafe { source_shape(&mut counter, 0) };
    assert_eq!(counter, 11);
}

#[test]
fn source_arm_1_preserves_owner() {
    let mut counter = 1;
    unsafe { source_shape(&mut counter, 1) };
    assert_eq!(counter, 11);
}

#[test]
fn source_arm_2_preserves_owner() {
    let mut counter = 1;
    unsafe { source_shape(&mut counter, 2) };
    assert_eq!(counter, 11);
}

#[cfg(feature = "baseline-ancestor")]
unsafe fn paired_shape(owner: &mut u64, older: *mut u64) {
    unsafe { callback(older) };
    *owner += 1;
}

#[cfg(not(feature = "baseline-ancestor"))]
fn paired_shape(owner: &mut u64) {
    unsafe { callback(std::ptr::from_mut(owner)) };
    *owner += 1;
}

#[test]
fn paired_argument_preserves_owner() {
    let mut counter = 1;
    #[cfg(feature = "baseline-ancestor")]
    {
        let older = std::ptr::from_mut(&mut counter);
        unsafe { paired_shape(&mut counter, older) };
    }
    #[cfg(not(feature = "baseline-ancestor"))]
    paired_shape(&mut counter);
    assert_eq!(counter, 12);
}

fn normalize_mut(owner: &mut u64) {
    *owner += 1;
}

fn old_apply_mut(owner: &mut u64) {
    let raw = owner as *mut u64;
    normalize_mut(owner);
    unsafe { callback(raw) };
}

fn fresh_apply_mut(owner: &mut u64) {
    normalize_mut(owner);
    unsafe { callback(owner as *mut u64) };
}

#[test]
fn apply_old_raw_before_mutable_owner_normalization() {
    let mut counter = 1;
    old_apply_mut(&mut counter);
    assert_eq!(counter, 12);
}

#[test]
fn apply_fresh_raw_after_mutable_owner_normalization() {
    let mut counter = 1;
    fresh_apply_mut(&mut counter);
    assert_eq!(counter, 12);
}

fn old_apply_read(owner: &mut u64) -> u64 {
    let raw = owner as *mut u64;
    let before = *owner;
    unsafe { callback(raw) };
    before
}

#[test]
fn apply_old_raw_before_plain_owner_read_control() {
    let mut counter = 1;
    assert_eq!(old_apply_read(&mut counter), 1);
    assert_eq!(counter, 11);
}

#[test]
fn direct_raw_callback_control() {
    let mut counter = 1;
    unsafe { callback(&mut counter) };
    assert_eq!(counter, 11);
}
