use super::*;

#[test]
fn smoke_add_roundtrips_through_native_code() {
    // Proves the Cranelift toolchain compiles + runs native code in-build.
    assert_eq!(smoke_compile_add(40, 2).unwrap(), 42);
    assert_eq!(smoke_compile_add(0, 0).unwrap(), 0);
    assert_eq!(smoke_compile_add(-5, 5).unwrap(), 0);
    assert_eq!(smoke_compile_add(i64::MAX - 1, 1).unwrap(), i64::MAX);
}

#[test]
fn smoke_add_is_repeatable() {
    // Each call builds + tears down its own JITModule; doing it many times
    // must not leak, crash, or corrupt — a basic exec-memory lifecycle check.
    for i in 0..64 {
        assert_eq!(smoke_compile_add(i, 100).unwrap(), i + 100);
    }
}
