use crate::emacs_core::display_when::DisplayWhenContextExt;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn nil_and_t_are_taken_literally_without_evaluation() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = Value::make_buffer(eval.edit.buffers.current_buffer_id().expect("buffer"));
    assert!(
        !eval
            .display_when_form_holds(Value::NIL, buffer, 1, 1)
            .expect("no flow")
    );
    assert!(
        eval.display_when_form_holds(Value::symbol("t"), buffer, 1, 1)
            .expect("no flow")
    );
}

#[test]
fn an_error_in_form_counts_as_nil_like_dsafe_eval() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = Value::make_buffer(eval.edit.buffers.current_buffer_id().expect("buffer"));
    let erroring = eval.eval_str("'(car 1)").expect("read");
    assert!(
        !eval
            .display_when_form_holds(erroring, buffer, 1, 1)
            .expect("no flow")
    );
    // And leaves no binding behind.
    assert!(eval.eval_str("(boundp 'object)").expect("eval").is_nil());
}

#[test]
fn form_runs_with_redisplay_and_quit_inhibited_and_the_bindings_are_undone_on_error() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = Value::make_buffer(eval.edit.buffers.current_buffer_id().expect("buffer"));
    let probe = eval
        .eval_str("'(and inhibit-redisplay inhibit-quit)")
        .expect("read");
    assert!(
        eval.display_when_form_holds(probe, buffer, 1, 1)
            .expect("no flow")
    );
    let erroring = eval.eval_str("'(progn (car 1) t)").expect("read");
    assert!(
        !eval
            .display_when_form_holds(erroring, buffer, 1, 1)
            .expect("no flow")
    );
    assert!(eval.eval_str("inhibit-redisplay").expect("eval").is_nil());
    assert!(eval.eval_str("inhibit-quit").expect("eval").is_nil());
}

#[test]
fn with_display_buffer_current_selects_the_buffer_and_restores_the_callers() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let original = eval.edit.buffers.current_buffer_id().expect("buffer");
    let other = eval.edit.buffers.create_buffer("other");
    let seen = eval
        .with_display_buffer_current(other, |ctx| ctx.edit.buffers.current_buffer_id())
        .expect("live buffer");
    assert_eq!(seen, Some(other));
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(original));
}

/// GNU binds `object`, `position` and `buffer-position` at every site it
/// meets (src/xdisp.c:6152-6154): one FORM object on two spans is evaluated
/// twice, each time with that span's bindings, and the two answers are kept
/// apart.  (The port used to evaluate once per `equal` FORM with the first
/// occurrence's bindings, so `(when (> buffer-position 30) . "LATE")` on two
/// spans answered for both what it answered for the first.)

#[test]
fn a_form_cannot_collect_its_detached_replacement_before_classification() {
    use crate::emacs_core::display_when::DisplayWhenSite;
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buf_id = eval.edit.buffers.current_buffer_id().expect("buffer");
    let property = eval
        .eval_str(
            "(setq when-spec (cons 'when
           (cons '(progn (setcdr when-spec nil) (garbage-collect) t)
                 (list 'space :width 3))))",
        )
        .expect("self-modifying property");
    let sites = [DisplayWhenSite {
        property,
        object: Value::string(" "),
        position: 0,
        buffer_position: 1,
    }];
    let mut classified = false;
    eval.evaluate_display_when_sites(buf_id, None, &sites, |spec, _| {
        assert_eq!(
            spec.cons_car(),
            Value::symbol("space"),
            "saved replacement must survive collection"
        );
        classified = true;
        true
    })
    .expect("evaluation");
    assert!(classified);
}

#[test]
fn a_later_form_roots_the_replacement_installed_by_an_earlier_form() {
    use crate::emacs_core::display_when::DisplayWhenSite;
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buf_id = eval.edit.buffers.current_buffer_id().expect("buffer");
    let property = eval
        .eval_str(
            "(progn
           (setq when-later (cons 'when (cons t \"OLD\")))
           (list
             (cons 'when
               (cons '(progn
                        (setcdr when-later
                          (cons '(progn (setcdr when-later nil) (garbage-collect) t)
                                (list 'space :width 3)))
                        nil)
                     '(height 1)))
             when-later))",
        )
        .expect("mutating property");
    let sites = [DisplayWhenSite {
        property,
        object: Value::string(" "),
        position: 0,
        buffer_position: 1,
    }];
    let mut classified = false;
    eval.evaluate_display_when_sites(buf_id, None, &sites, |spec, _| {
        assert_eq!(
            spec.cons_car(),
            Value::symbol("space"),
            "newly installed replacement must survive collection"
        );
        classified = true;
        true
    })
    .expect("evaluation");
    assert!(classified);
}
