//! Actual front-owned sqrt scalar capture tests; no worker heap access.

use super::compile_pipeline_tests::function;
use super::*;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            sink: true,
            ..OptPasses::default()
        }));
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
    }
}
fn capture(ctx: &Context, symbol: Value) -> super::sqrt_snapshot::SqrtCallWitnesses {
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![symbol],
        1,
    );
    let cfg = analyze_cfg(
        f.executable_ops(),
        &f.constants,
        f.executable_gnu_byte_offset_map(),
        1,
    )
    .unwrap();
    let sites = find_spec_sites(
        f.executable_ops(),
        &f.constants,
        &cfg.leaders,
        &ctx.obarray,
        true,
    );
    super::sqrt_snapshot::capture(f.executable_ops(), &sites, &ctx.obarray)
}
#[test]
fn opt_sqrt_snapshot_uses_actual_subr_entry_not_symbol_spelling_and_is_owned_scalar() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let sqrt = crate::emacs_core::intern::intern("sqrt");
    let alias = crate::emacs_core::intern::intern("t34-real-sqrt-alias");
    let original = ctx.obarray.symbol_function_id(sqrt).unwrap();
    ctx.obarray.set_symbol_function_id(alias, original);
    let actual = capture(&ctx, Value::from_sym_id(sqrt));
    let aliased = capture(&ctx, Value::from_sym_id(alias));
    assert_eq!(actual.len(), 1);
    assert_eq!(aliased.len(), 1);
    assert_eq!(actual[&2].expected_subr_bits, original.bits() as u64);
    assert_eq!(aliased[&2].expected_subr_bits, original.bits() as u64);
    assert_ne!(actual[&2].symbol_bits, aliased[&2].symbol_bits);
    // The name sqrt now resolves to a real plain A1 builtin with a different
    // function entry. Kind/arity alone and spelling alone must not certify it.
    let sin = ctx
        .obarray
        .symbol_function_id(crate::emacs_core::intern::intern("sin"))
        .unwrap();
    ctx.obarray.set_symbol_function_id(sqrt, sin);
    assert!(capture(&ctx, Value::from_sym_id(sqrt)).is_empty());
    ctx.obarray.set_symbol_function_id(sqrt, original);
    let replacement_epoch = ctx.obarray.function_epoch();
    assert_ne!(actual[&2].epoch, replacement_epoch);
    // Actual worker closure owns only scalar compiler metadata and reads no
    // Context, Subr, Lisp object or runtime TLS on the new thread.
    let worker = std::thread::spawn(move || (actual[&2], aliased[&2]));
    let (actual, aliased) = worker.join().unwrap();
    assert_eq!(actual.expected_subr_bits, aliased.expected_subr_bits);
    assert_ne!(actual.epoch, replacement_epoch);
    force_opt_passes_for_test(Some(OptPasses::default()));
    assert!(capture(&ctx, Value::from_sym_id(sqrt)).is_empty());
}
