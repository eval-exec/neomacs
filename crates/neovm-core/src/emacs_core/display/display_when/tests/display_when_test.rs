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

/// GNU takes FORM as true without evaluating it only when it is `t' itself
/// (`EQ (form, Qt)', src/xdisp.c:6143-6145).  An uninterned symbol named `t'
/// is an ordinary variable: it is evaluated, and its value decides.
#[test]
fn an_uninterned_symbol_named_t_is_evaluated_not_taken_as_t() {
    let mut eval = Context::new();
    let buffer = Value::make_buffer(eval.buffers.current_buffer_id().expect("buffer"));
    let unbound = eval.eval_str("(make-symbol \"t\")").expect("make-symbol");
    assert!(
        !eval
            .display_when_form_holds(unbound, buffer, 1, 1)
            .expect("no flow"),
        "an unbound #:t signals void-variable, which counts as nil"
    );
    let bound_to_nil = eval
        .eval_str("(let ((s (make-symbol \"t\"))) (set s nil) s)")
        .expect("bound #:t");
    assert!(
        !eval
            .display_when_form_holds(bound_to_nil, buffer, 1, 1)
            .expect("no flow"),
        "a #:t whose value is nil does not hold"
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
fn sites_are_evaluated_once_per_site_with_the_buffer_current_and_survive_gc() {
    use crate::emacs_core::display_when::{DisplayWhenSite, DisplayWhenSiteKey};
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
        .evaluate_display_when_sites(other, None, &sites, |spec, _| spec.is_string())
        .expect("no flow");
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(collecting, &sites[0])),
        Some(&true)
    );
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(named, &sites[1])),
        Some(&true)
    );
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(named, &sites[2])),
        None,
        "a disable-eval site has no answer"
    );
    assert_eq!(results.len(), 2);
    assert_eq!(eval.buffers.current_buffer_id(), Some(original));
}

/// GNU binds `object`, `position` and `buffer-position` at every site it
/// meets (src/xdisp.c:6152-6154): one FORM object on two spans is evaluated
/// twice, each time with that span's bindings, and the two answers are kept
/// apart.  (The port used to evaluate once per `equal` FORM with the first
/// occurrence's bindings, so `(when (> buffer-position 30) . "LATE")` on two
/// spans answered for both what it answered for the first.)
#[test]
fn one_form_on_two_sites_is_evaluated_at_each_with_its_own_bindings() {
    use crate::emacs_core::display_when::{DisplayWhenSite, DisplayWhenSiteKey};
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("buffer");
    let buffer = Value::make_buffer(buf_id);
    eval.eval_str("(setq dw-seen nil)").expect("reset");
    let form = eval
        // A bare `Context` has no `push' macro: subrs only.
        .eval_str(
            "'(progn (setq dw-seen (cons (list position buffer-position) dw-seen)) (> buffer-position 30))",
        )
        .expect("read");
    let property = Value::cons(
        Value::symbol("when"),
        Value::cons(form, Value::string("LATE")),
    );
    let sites = [
        DisplayWhenSite {
            property,
            object: buffer,
            position: 1,
            buffer_position: 1,
        },
        DisplayWhenSite {
            property,
            object: buffer,
            position: 36,
            buffer_position: 36,
        },
    ];
    let results = eval
        .evaluate_display_when_sites(buf_id, None, &sites, |spec, _| spec.is_string())
        .expect("no flow");
    assert_eq!(results.len(), 2);
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(form, &sites[0])),
        Some(&false)
    );
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(form, &sites[1])),
        Some(&true)
    );
    let seen = eval.eval_str("(reverse dw-seen)").expect("seen");
    let expected = eval.eval_str("'((1 1) (36 36))").expect("read");
    assert!(
        eval.eval_str("(equal (reverse dw-seen) '((1 1) (36 36)))")
            .expect("compare")
            .is_truthy(),
        "each site evaluated once with its own bindings: {seen:?} vs {expected:?}"
    );
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
