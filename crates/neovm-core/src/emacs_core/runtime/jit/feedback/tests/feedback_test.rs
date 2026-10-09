//! The call-target lattice, the call-site table and the knob (P2.1 C3/C4).

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::LambdaParams;

/// A fresh source: a nullary function returning its constant.
fn source(tag: i64) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.ops = vec![Op::Constant(0), Op::Return];
    f.constants = vec![Value::make_int(tag)].into();
    f.max_stack = 1;
    f
}

fn site() -> CallSiteFeedback {
    CallSiteFeedback::new(3, SiteShape::Callee)
}

#[cfg(feature = "jit")]
fn sources_of(site: &CallSiteFeedback) -> Vec<*const RuntimeState> {
    match site.target() {
        CallTarget::Sources(v) => v.iter().map(Arc::as_ptr).collect(),
        other => panic!("expected sources, got {other:?}"),
    }
}

#[test]
fn a_symbol_site_is_monomorphic_until_a_second_symbol() {
    let s = site();
    assert!(matches!(s.target(), CallTarget::Uninit));
    let a = Value::symbol("feedback-test-a");
    s.observe(a);
    s.observe(a);
    assert!(matches!(s.target(), CallTarget::Sym(id) if Some(id) == a.as_symbol_id()));
    s.observe(Value::symbol("feedback-test-b"));
    assert!(matches!(s.target(), CallTarget::Mega));
    // Mega is final.
    s.observe(a);
    assert!(matches!(s.target(), CallTarget::Mega));
}

#[cfg(feature = "jit")]
#[test]
fn instances_of_one_source_are_one_target_and_sources_go_poly_then_mega() {
    let s = site();
    let f1 = Value::make_bytecode(source(1));
    // A `make-closure` instance shares its prototype's runtime.
    let proto = source(2);
    let i1 = Value::make_bytecode(proto.clone());
    let i2 = Value::make_bytecode(proto.clone());
    s.observe(i1);
    s.observe(i2);
    assert_eq!(sources_of(&s), vec![proto.jit_runtime().state_ptr()]);
    s.observe(f1);
    assert_eq!(
        sources_of(&s),
        vec![
            proto.jit_runtime().state_ptr(),
            f1.get_bytecode_data().unwrap().jit_runtime().state_ptr()
        ]
    );
    // Seen sources keep the site at its arity.
    s.observe(i1);
    assert_eq!(sources_of(&s).len(), 2);
    let more: Vec<Value> = (3..5).map(|t| Value::make_bytecode(source(t))).collect();
    for f in &more {
        s.observe(*f);
    }
    assert_eq!(sources_of(&s).len(), POLY);
    s.observe(Value::make_bytecode(source(9)));
    assert!(matches!(s.target(), CallTarget::Mega), "a fifth source");
}

#[test]
fn mixing_symbols_sources_and_other_callees_is_megamorphic() {
    let f = Value::make_bytecode(source(1));
    let sym = Value::symbol("feedback-test-mix");
    for (first, second) in [(f, sym), (sym, f), (f, Value::make_int(3))] {
        let s = site();
        s.observe(first);
        s.observe(second);
        assert!(
            matches!(s.target(), CallTarget::Mega),
            "{first:?} then {second:?}"
        );
    }
    let s = site();
    s.observe(Value::make_int(3));
    assert!(matches!(s.target(), CallTarget::Mega), "a non-function");
}

/// The site holds its sources weakly: it keeps no function alive, and a
/// source that died reads as gone (all gone = megamorphic).
#[cfg(feature = "jit")]
#[test]
fn a_site_holds_its_sources_weakly_and_releases_them() {
    let rt = crate::emacs_core::jit::Runtime::new();
    let state = rt.share_state();
    assert_eq!(Arc::weak_count(&state), 0);
    let mut f = source(1);
    f.runtime = Some(rt);
    let v = Value::make_bytecode(f);
    {
        let s = site();
        s.observe(v);
        assert_eq!(Arc::weak_count(&state), 1, "the site's Weak");
        assert_eq!(Arc::strong_count(&state), 2, "no strong hold");
        assert_eq!(sources_of(&s), vec![Arc::as_ptr(&state)]);
    }
    assert_eq!(Arc::weak_count(&state), 0, "released with the site");
    // A source that died drops out of a compile's view; all gone reads Mega.
    let live = Arc::new(RuntimeState::new());
    let dead = Arc::new(RuntimeState::new());
    let s = site();
    s.sources[0].store(
        Weak::into_raw(Arc::downgrade(&live)).cast_mut(),
        Ordering::Relaxed,
    );
    s.sources[1].store(
        Weak::into_raw(Arc::downgrade(&dead)).cast_mut(),
        Ordering::Relaxed,
    );
    s.state.store((2 << 3) | TAG_POLY, Ordering::Relaxed);
    drop(dead);
    assert_eq!(sources_of(&s), vec![Arc::as_ptr(&live)]);
    drop(live);
    assert!(matches!(s.target(), CallTarget::Mega), "every source died");
}

/// A counted transition after the window is late; before it, it is not.
#[test]
fn transitions_after_the_window_count_as_late() {
    let s = site();
    let a = Value::symbol("feedback-test-late-a");
    for _ in 0..STABLE_WINDOW {
        s.observe_counted(a);
    }
    assert_eq!(s.count(), STABLE_WINDOW);
    assert_eq!(s.late(), 0);
    s.observe_counted(Value::symbol("feedback-test-late-b"));
    assert_eq!(s.late(), 1);
    let early = site();
    early.observe_counted(a);
    early.observe_counted(Value::symbol("feedback-test-late-b"));
    assert_eq!(early.late(), 0);
}

/// The table has a site where the callee is not a constant, and at the
/// constant `apply` and mapping builtins (argument 0); a constant callee
/// the compile reads records nothing.
#[cfg(feature = "jit")]
#[test]
fn the_table_classifies_call_sites_by_their_callee() {
    let constants: Vec<Value> = vec![
        Value::symbol("feedback-test-named"),
        Value::from_sym_id(intern("apply")),
        Value::from_sym_id(intern("mapcar")),
    ];
    let ops = vec![
        // 0..=2: (feedback-test-named x) -- constant
        Op::Constant(0),
        Op::StackRef(1),
        Op::Call(1),
        // 3..=5: (funcall x) -- the callee is an argument
        Op::StackRef(1),
        Op::StackRef(2),
        Op::Call(1),
        // 6..=9: (apply x x)
        Op::Constant(1),
        Op::StackRef(3),
        Op::StackRef(4),
        Op::Call(2),
        // 10..=13: (mapcar x x)
        Op::Constant(2),
        Op::StackRef(4),
        Op::StackRef(5),
        Op::Call(2),
        Op::Return,
    ];
    let table = CallSites::build(&ops, &constants);
    assert!(table.site_at(2).is_none(), "a constant callee");
    assert_eq!(
        table.site_at(5).map(CallSiteFeedback::shape),
        Some(SiteShape::Callee)
    );
    assert_eq!(
        table.site_at(9).map(CallSiteFeedback::shape),
        Some(SiteShape::ApplyArg)
    );
    assert_eq!(
        table.site_at(13).map(CallSiteFeedback::shape),
        Some(SiteShape::CallbackArg)
    );
    assert!(table.site_at(0).is_none() && table.site_at(99).is_none());
    assert_eq!(table.sites().len(), 3);
    // An apply site records its argument 0, a callee site its callee.
    let f = Value::make_bytecode(source(1));
    let sym = Value::symbol("feedback-test-arg0");
    let apply_site = table.site_at(9).unwrap();
    assert_eq!(apply_site.target_of(sym, f).bits(), f.bits());
    let callee_site = table.site_at(5).unwrap();
    assert_eq!(callee_site.target_of(sym, f).bits(), sym.bits());
}

/// A jump target resets the scan: a callee pushed before a join is not
/// known to be the constant after it.
#[test]
fn a_block_boundary_forgets_the_constant_callee() {
    let constants = vec![Value::symbol("feedback-test-joined")];
    let ops = vec![
        Op::Constant(0),
        Op::StackRef(1),
        Op::GotoIfNil(4),
        Op::Nil,
        // 4: a leader
        Op::Call(0),
        Op::Return,
    ];
    let table = CallSites::build(&ops, &constants);
    assert_eq!(
        table.site_at(4).map(CallSiteFeedback::shape),
        Some(SiteShape::Callee)
    );
}

#[test]
fn the_knob_parses_its_three_values() {
    assert_eq!(FeedbackMode::parse("off"), Some(FeedbackMode::Off));
    assert_eq!(FeedbackMode::parse("record"), Some(FeedbackMode::Record));
    assert_eq!(FeedbackMode::parse("use"), Some(FeedbackMode::Use));
    assert_eq!(FeedbackMode::parse("census"), Some(FeedbackMode::Census));
    assert!(FeedbackMode::Census.records() && !FeedbackMode::Census.uses());
    assert!(!FeedbackMode::Census.windowed() && FeedbackMode::Record.windowed());
    // Compiled sites record only in the recording modes, not under `use`.
    assert!(FeedbackMode::Record.records_compiled() && FeedbackMode::Census.records_compiled());
    assert!(!FeedbackMode::Use.records_compiled() && !FeedbackMode::Off.records_compiled());
    assert_eq!(FeedbackMode::parse("bogus"), None);
    assert!(!FeedbackMode::Off.records());
    assert!(FeedbackMode::Record.records() && !FeedbackMode::Record.uses());
    assert!(FeedbackMode::Use.records() && FeedbackMode::Use.uses());
    force_feedback_mode_for_test(Some(FeedbackMode::Use));
    assert_eq!(feedback_mode(), FeedbackMode::Use);
    force_feedback_mode_for_test(None);
}

/// The census renders its summary and ranks the executed sites.
#[cfg(feature = "jit")]
#[test]
fn the_census_renders_the_window_and_late_counts() {
    use crate::emacs_core::jit::stats::calls::{SiteRow, StateKind, render};
    let row = |owner: &str, kind, count, late| SiteRow {
        owner: owner.to_string(),
        pc: 7,
        shape: "callee",
        state: "x".to_string(),
        kind,
        count,
        late,
    };
    let lines = render(
        2,
        vec![
            row("a", StateKind::Source, STABLE_WINDOW + 5, 0),
            row("b", StateKind::Mega, STABLE_WINDOW, 1),
            row("c", StateKind::Sym, 3, 0),
            row("d", StateKind::Uninit, 0, 0),
        ],
    );
    let summary = &lines[0].1;
    for want in [
        "sources=2",
        "sites=4",
        "state[uninit=1 sym=1 source=1 poly=0 mega=1]",
        "counted=3",
        "window_sites=2",
        "late_sites=1",
        "late_transitions=1",
        "window_mono_poly=2",
        "late_mono_poly=1",
    ] {
        assert!(summary.contains(want), "{want} in {summary}");
    }
    assert_eq!(lines.len(), 4, "three executed sites ranked");
    assert!(lines[1].1.starts_with("fn=a pc=7"), "{}", lines[1].1);
}
