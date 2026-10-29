//! GNU's chosen catch policy survives rooted loader/public error boundaries.
use super::{Flow, collect_in_flight_flow_gc_roots, flow_from_eval_error};
use crate::emacs_core::print::map_flow;
use crate::emacs_core::{Context, Value, print::print_value};
use crate::tagged::SymbolMode;

#[test]
fn issue554_throw_policy_survives_real_load_and_flag_unbinding() {
    // GNU eval.c:1477 selects using EQ before eval.c:1449 unbinds;
    // lread.c:2338-2340 evaluates loaded forms in the active handler chain.
    let fixtures = tempfile::tempdir().expect("private load fixture directory");
    let file = fixtures.path().join("throw-policy.el");
    for (source, catch_tag, expected) in [
        (
            "(let ((symbols-with-pos-enabled t)) (throw (position-symbol 'issue554-load-tag 0) 7))",
            "'issue554-load-tag",
            "(7 nil)",
        ),
        (
            "(let ((symbols-with-pos-enabled t)) (throw 'issue554-load-tag 8))",
            "(position-symbol 'issue554-load-tag 9)",
            "(8 nil)",
        ),
        (
            "(let ((symbols-with-pos-enabled nil)) (throw (position-symbol 'issue554-load-tag 0) 7))",
            "'issue554-load-tag",
            "(no-catch nil)",
        ),
    ] {
        std::fs::write(
            &file,
            format!(";;; -*- lexical-binding: nil -*-\n{source}\n"),
        )
        .expect("write exclusively owned fixture");
        let mut ctx = Context::new();
        let path = print_value(&Value::string(file.to_str().expect("UTF-8 fixture path")));
        let form = format!(
            "(let ((symbols-with-pos-enabled nil))
               (list (condition-case err
                         (catch {catch_tag} (load {path} nil t t) 'fell-through)
                       (no-catch (car err)))
                     symbols-with-pos-enabled))"
        );
        let value = ctx.eval_str(&form).expect("load/catch policy fixture");
        assert_eq!(print_value(&value), expected, "{source}");
    }
}

#[test]
fn issue554_cloned_owned_throw_roundtrip_preserves_policy_and_roots() {
    for policy in [SymbolMode::Bare, SymbolMode::Positioned] {
        let mut ctx = Context::new();
        let bare_tag = Value::symbol("issue554-cloned-throw-tag");
        let tag = ctx
            .tagged_heap
            .alloc_symbol_with_pos(bare_tag, Value::fixnum(5));
        let payload = Value::vector(vec![Value::fixnum(7)]);
        let owned = map_flow(Flow::throw_with_symbols(tag, payload, policy));
        let cloned = owned.clone();
        drop(owned);
        // Current Lisp policy must not replace the throw's already selected
        // equality. Collection has only the cloned owned error's pin here.
        ctx.set_variable("symbols-with-pos-enabled", Value::NIL);
        let mut roots = Vec::new();
        collect_in_flight_flow_gc_roots(&mut roots, ctx.tagged_heap.identity());
        assert!(roots.iter().any(|root| root.bits() == tag.bits()));
        assert!(roots.iter().any(|root| root.bits() == payload.bits()));
        ctx.gc_collect_exact();
        let roundtrip = flow_from_eval_error(cloned);
        let thrown = roundtrip.as_throw().expect("owned error stays a throw");
        assert_eq!(thrown.symbol_mode(), policy);
        assert_eq!(
            thrown.matches_tag(bare_tag),
            policy == SymbolMode::Positioned
        );
        assert!(
            thrown.matches_tag(tag),
            "identical tags match with either policy"
        );
        ctx.gc_collect_exact();
        assert_eq!(thrown.value.as_vector_data().unwrap()[0], Value::fixnum(7));
        assert_eq!(thrown.tag.as_symbol_with_pos_sym(), Some(bare_tag));
        drop(roundtrip);
        let mut roots = Vec::new();
        collect_in_flight_flow_gc_roots(&mut roots, ctx.tagged_heap.identity());
        assert!(roots.iter().all(|root| root.bits() != tag.bits()));
        assert!(roots.iter().all(|root| root.bits() != payload.bits()));
    }
}

#[test]
fn issue554_throw_policy_survives_bytecode_loader_frame() {
    use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
    use crate::emacs_core::intern::intern;
    use crate::emacs_core::value::LambdaParams;

    // A real image invokes load-with-code-conversion in bytecode; minimal
    // Context loading lacks that intervening frame. Model the same boundary
    // with a compiled shim calling an interpreted loaded-form evaluator.
    crate::emacs_core::jit::force_jit_off_for_test(true);
    for (body, catch_tag, expected) in [
        (
            "(let ((symbols-with-pos-enabled t)) (throw (position-symbol 'issue554-loader-tag 0) 7))",
            "'issue554-loader-tag",
            "(7 nil)",
        ),
        (
            "(let ((symbols-with-pos-enabled t)) (throw 'issue554-loader-tag 8))",
            "(position-symbol 'issue554-loader-tag 9)",
            "(8 nil)",
        ),
        (
            "(let ((symbols-with-pos-enabled nil)) (throw (position-symbol 'issue554-loader-tag 0) 7))",
            "'issue554-loader-tag",
            "(no-catch nil)",
        ),
    ] {
        let mut ctx = Context::new();
        ctx.eval_str(&format!("(fset 'issue554-loaded-form '(lambda () {body}))"))
            .expect("interpreted loaded form");
        let mut shim = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        shim.ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
        shim.constants = vec![Value::symbol("issue554-loaded-form")].into();
        shim.max_stack = StackDepth::for_test(4);
        shim.seal_hand_assembled_ops_for_test();
        ctx.obarray.set_symbol_function_id(
            intern("issue554-compiled-loader"),
            Value::make_bytecode(shim),
        );
        let value = ctx
            .eval_str(&format!(
                "(let ((symbols-with-pos-enabled nil))
               (list (condition-case err
                         (catch {catch_tag} (issue554-compiled-loader) 'fell-through)
                       (no-catch (car err)))
                     symbols-with-pos-enabled))"
            ))
            .expect("throw through compiled loader frame");
        assert_eq!(print_value(&value), expected, "{body}");
    }
}
