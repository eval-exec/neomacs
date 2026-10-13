use neovm_core::emacs_core::Context;

fn raw_compatibility_phase(context: &mut Context) {
    let mut writer = context.obarray_mut();
    // SAFETY: this deliberate compile-fail attempt cannot name the private phase;
    // no successful unsafe operation or executable is produced.
    unsafe { writer.with_legacy_storage_write(|manager| manager.fmakunbound("probe")) };
}

fn main() {}
