use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::compile::{NativeRun, lower_nullary_leaf, take_pending_flow};
use neovm_value::error::FlowKind;
use neovm_value::value::Value;

#[test]
fn contained_panic_in_load_unwinds_load_bookkeeping() {
    // `load` bookkeeping rides the specpdl: a panic contained mid-load
    // must leave `load-in-progress` nil and `loads_in_progress` empty
    // once the deferred unwind runs, and repeated containment must not
    // accumulate entries into a spurious "Recursive load".
    let mut ev = crate::test_utils::new_context();
    let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("jit-fx-panic-load.el");
    std::fs::write(&fixture, "(neovm--internal-panic \"load-boom\")\n")
        .expect("write load fixture");
    let path_str = fixture.to_string_lossy().into_owned();
    let var = Value::symbol("jit-fx-load-dynvar");
    let leaf = lower_nullary_leaf(
        &[
            Op::Constant(1), // 5
            Op::VarBind(0),  // has_binds: exit parity unwind sweeps the leak
            Op::Constant(2), // 'load
            Op::Constant(3), // absolute fixture path
            Op::Call(1),
            Op::Unbind(1),
            Op::Return,
        ],
        &[
            var,
            Value::make_int(5),
            Value::symbol("load"),
            Value::string(&path_str),
        ],
    )
    .expect("load call body compiles");
    let spec0 = ev.specpdl().len();
    // GNU signals "Recursive load" once the same file is in flight five
    // times; five leaked entries would previously get there. Every round
    // must instead report the contained panic with clean state.
    for round in 0..5 {
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "round {round}");
        let flow = take_pending_flow().expect("panic flow stashed");
        let sig = match flow.into_kind() {
            FlowKind::Signal(sig) => sig,
            flow => {
                panic!("round {round}: expected Signal, got {flow:?}");
            }
        };
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(
            msg.contains("load-boom") && !msg.contains("Recursive load"),
            "round {round}: unexpected message: {msg}"
        );
        assert!(
            ev.loads_in_progress().is_empty(),
            "round {round}: loads_in_progress leaked"
        );
        assert_eq!(
            ev.obarray()
                .symbol_value_copied("load-in-progress")
                .unwrap_or(Value::NIL),
            Value::NIL,
            "round {round}: load-in-progress wedged"
        );
        assert_eq!(ev.specpdl().len(), spec0, "round {round}: specpdl swept");
    }
    // And a healthy load of a well-formed file still works afterwards.
    let ok_file = dir.path().join("jit-fx-ok-load.el");
    std::fs::write(&ok_file, "(setq jit-fx-load-ok t)\n").expect("write ok fixture");
    ev.eval_str(&format!("(load {:?} nil t)", ok_file.to_string_lossy()))
        .expect("normal load succeeds after repeated containment");
    assert!(ev.loads_in_progress().is_empty());
}
