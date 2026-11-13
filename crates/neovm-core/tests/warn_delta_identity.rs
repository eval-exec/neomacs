//! Warn-delta P2 row 4 oracles: GNU 31.1 defines `file-user-uid` and
//! `file-group-gid` as Lisp functions, not as subrs — lisp/simple.el:4826 and
//! :4838 dispatch through `find-file-name-handler` on `default-directory` and
//! fall back to `user-uid` / `group-gid`. These tests pin the GNU Lisp layer
//! (the layer the deleted dead Rust helpers duplicated) in a booted runtime.

fn booted() -> neovm_eval::emacs_core::eval::Context {
    neovm_core::emacs_core::load_boot::create_runtime_startup_evaluator_cached()
        .expect("runtime startup evaluator")
}

fn eval_rendered(eval: &mut neovm_eval::emacs_core::eval::Context, source: &str) -> String {
    format!(
        "{}",
        eval.eval_str(source)
            .expect("Lisp evaluation should succeed")
    )
}

#[test]
fn file_user_uid_matches_user_uid() {
    let mut eval = booted();
    assert_eq!(
        eval_rendered(
            &mut eval,
            "(and (= (file-user-uid) (user-uid)) (integerp (file-user-uid)))"
        ),
        "t"
    );
}

#[test]
fn file_user_uid_arity_errors() {
    let mut eval = booted();
    assert_eq!(
        eval_rendered(
            &mut eval,
            "(condition-case nil (file-user-uid nil) (wrong-number-of-arguments t))"
        ),
        "t"
    );
}

#[test]
fn file_group_gid_matches_group_gid() {
    let mut eval = booted();
    assert_eq!(
        eval_rendered(
            &mut eval,
            "(and (= (file-group-gid) (group-gid)) (integerp (file-group-gid)))"
        ),
        "t"
    );
}

#[test]
fn file_group_gid_arity_errors() {
    let mut eval = booted();
    assert_eq!(
        eval_rendered(
            &mut eval,
            "(condition-case nil (file-group-gid nil) (wrong-number-of-arguments t))"
        ),
        "t"
    );
}
