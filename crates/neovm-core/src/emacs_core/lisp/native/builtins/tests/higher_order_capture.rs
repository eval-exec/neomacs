use super::*;
use crate::tagged::collection_reads::capture;

fn list_cell(mut list: Value, index: usize) -> Value {
    for _ in 0..index {
        list = list.cons_cdr();
    }
    list
}

#[test]
fn higher_order_capture_records_mapcar_traversal_and_whole_call() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    for whole_call in [false, true] {
        for index in 0..3 {
            let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
            let source = list_cell(list, index);
            let roots = eval.save_vm_roots();
            eval.push_vm_frame_root(list);
            let (result, reads) = capture(|| {
                if whole_call {
                    builtin_mapcar_2(&mut eval, Value::symbol("identity"), list).map(|_| ())
                } else {
                    // Exercise the traversal independently: a length pass
                    // inside the capture would already observe each cell.
                    mapcar1_eval(&mut eval, 3, MapSink::Discard, list, |_, item| Ok(item))
                        .map(|_| ())
                }
            });
            eval.restore_vm_roots(roots);
            assert!(result.is_ok());
            let reads = reads.expect("pure mapcar has coherent reads");
            assert!(reads.unchanged());
            source.set_car(Value::fixnum(99));
            assert!(!reads.unchanged(), "whole_call={whole_call}, cell={index}");
        }
    }
}

#[test]
fn higher_order_capture_callback_observes_its_own_nested_reads() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let sources = [
        Value::cons(Value::fixnum(1), Value::NIL),
        Value::cons(Value::fixnum(2), Value::NIL),
        Value::cons(Value::fixnum(3), Value::NIL),
    ];
    let list = Value::list(sources.to_vec());
    let roots = eval.save_vm_roots();
    eval.push_vm_frame_root(list);
    let mut callback_reads = Vec::new();
    assert!(!crate::tagged::collection_reads::is_active());
    // With the hoisting knob enabled, the outer traversal selects its
    // unobserved clone. The callback's ordinary accessors must nevertheless
    // observe the nested scope, which finishes before traversal resumes.
    let result = mapcar1_eval(&mut eval, 3, MapSink::Discard, list, |_, item| {
        let (value, reads) = capture(|| item.cons_car());
        callback_reads.push(reads.expect("callback capture is coherent"));
        Ok(value)
    });
    eval.restore_vm_roots(roots);
    assert_eq!(result.unwrap(), sources.len());
    assert!(!crate::tagged::collection_reads::is_active());
    for reads in &callback_reads {
        assert!(reads.unchanged());
    }
    for (source, reads) in sources.into_iter().zip(callback_reads) {
        source.set_car(Value::T);
        assert!(!reads.unchanged(), "callback's source must be observed");
    }
}

#[test]
fn higher_order_capture_records_mapconcat_identity_list_cells() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    for whole_call in [false, true] {
        for index in 0..3 {
            let list = Value::list(vec![
                Value::string("a"),
                Value::string("b"),
                Value::string("c"),
            ]);
            let source = list_cell(list, index);
            let (result, reads) = capture(|| {
                if whole_call {
                    builtin_mapconcat(&mut eval, vec![Value::symbol("identity"), list]).map(|_| ())
                } else {
                    let mut parts = MapResultVec::new();
                    mapconcat_identity_list(list, &mut parts);
                    Ok(())
                }
            });
            assert!(result.is_ok());
            let reads = reads.expect("identity traversal has no source writes");
            source.set_car(Value::NIL);
            assert!(!reads.unchanged(), "whole_call={whole_call}, cell={index}");
        }
    }
}
