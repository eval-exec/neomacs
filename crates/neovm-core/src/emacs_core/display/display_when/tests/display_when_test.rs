use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn nil_and_t_are_taken_literally_without_evaluation() {
    let mut eval = Context::new();
    let buffer = Value::make_buffer(eval.buffers.current_buffer_id().expect("buffer"));
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
fn form_sees_object_position_and_buffer_position_bound_dynamically() {
    let mut eval = Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("buffer");
    let buffer = Value::make_buffer(buffer_id);
    let probe = eval
        .eval_str("'(and (bufferp object) (= position 7) (= buffer-position 42))")
        .expect("read");
    assert!(
        eval.display_when_form_holds(probe, buffer, 7, 42)
            .expect("no flow")
    );
    let other = eval.eval_str("'(= position 8)").expect("read");
    assert!(
        !eval
            .display_when_form_holds(other, buffer, 7, 42)
            .expect("no flow")
    );
    // The bindings are undone afterwards (GNU `unbind_to`).
    assert!(eval.eval_str("(boundp 'position)").expect("eval").is_nil());
    assert!(
        eval.eval_str("(boundp 'buffer-position)")
            .expect("eval")
            .is_nil()
    );
}

#[test]
fn an_error_in_form_counts_as_nil_like_dsafe_eval() {
    let mut eval = Context::new();
    let buffer = Value::make_buffer(eval.buffers.current_buffer_id().expect("buffer"));
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
    let mut eval = Context::new();
    let buffer = Value::make_buffer(eval.buffers.current_buffer_id().expect("buffer"));
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
    let mut eval = Context::new();
    let original = eval.buffers.current_buffer_id().expect("buffer");
    let other = eval.buffers.create_buffer("other");
    let seen = eval
        .with_display_buffer_current(other, |ctx| ctx.buffers.current_buffer_id())
        .expect("live buffer");
    assert_eq!(seen, Some(other));
    assert_eq!(eval.buffers.current_buffer_id(), Some(original));
}

#[test]
fn sites_are_evaluated_once_per_form_with_the_buffer_current_and_survive_gc() {
    use crate::emacs_core::display_when::DisplayWhenSite;
    let mut eval = Context::new();
    let original = eval.buffers.current_buffer_id().expect("buffer");
    let other = eval.buffers.create_buffer("other");
    let buffer = Value::make_buffer(other);
    // The first form collects; the second must still be intact afterwards.
    let collecting = eval.eval_str("'(progn (garbage-collect) t)").expect("read");
    let named = eval
        .eval_str("'(string-equal (buffer-name) \"other\")")
        .expect("read");
    let property = |form| Value::cons(Value::symbol("when"), Value::cons(form, Value::string("X")));
    let sites = [
        DisplayWhenSite {
            property: property(collecting),
            object: buffer,
            position: 1,
            buffer_position: 1,
        },
        DisplayWhenSite {
            property: property(named),
            object: buffer,
            position: 2,
            buffer_position: 2,
        },
        DisplayWhenSite {
            property: Value::list(vec![Value::symbol("disable-eval"), property(named)]),
            object: buffer,
            position: 3,
            buffer_position: 3,
        },
    ];
    let results = eval
        .evaluate_display_when_sites(other, None, &sites, |spec| spec.is_string())
        .expect("no flow");
    assert_eq!(results.get(&collecting), Some(&true));
    assert_eq!(results.get(&named), Some(&true), "first occurrence decides");
    assert_eq!(results.len(), 2);
    assert_eq!(eval.buffers.current_buffer_id(), Some(original));
}

#[test]
fn a_form_cannot_collect_its_detached_replacement_before_classification() {
    use crate::emacs_core::display_when::DisplayWhenSite;
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
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
    eval.evaluate_display_when_sites(buf_id, None, &sites, |spec| {
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
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
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
    eval.evaluate_display_when_sites(buf_id, None, &sites, |spec| {
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
