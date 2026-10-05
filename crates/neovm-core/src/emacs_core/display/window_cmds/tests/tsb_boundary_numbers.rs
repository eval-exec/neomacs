//! Expectations captured from GNU Emacs 31.1 through sandbox-run.sh.
use crate::test_utils::runtime_startup_eval_one;

#[test]
fn tsb_window_pixel_sizes_check_the_result_before_mutation() {
    let form = r#"(list
      (condition-case e (set-window-new-pixel nil most-positive-fixnum) (error e))
      (condition-case e (set-window-new-pixel nil -1) (error e))
      (set-window-new-pixel nil 12)
      (set-window-new-pixel nil -12 t)
      (condition-case e (set-window-new-pixel nil -1 t) (error e))
      (set-window-new-pixel nil 2147483647)
      (condition-case e (set-window-new-pixel nil 1 t) (error e))
      (condition-case e (set-window-new-pixel nil (ash 1 65)) (error (car e)))
      (window-new-pixel nil))"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-window-pixel.expect").trim_end()
    );
}

#[test]
fn tsb_split_rejects_extreme_sizes_without_mutating_the_tree() {
    let form = r#"(list
      (condition-case e (split-window-internal nil most-positive-fixnum nil nil) (error e))
      (condition-case e (split-window-internal nil most-negative-fixnum nil nil) (error e))
      (condition-case e (split-window-internal nil 0 nil nil) (error e))
      (condition-case e (split-window-internal nil 1 t nil) (error e))
      (length (window-list)))"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-window-split.expect").trim_end()
    );
}

#[test]
fn tsb_window_total_addition_keeps_gnu_signed_fixnum_slots() {
    let form = r#"(list
      (set-window-new-total nil most-positive-fixnum)
      (set-window-new-total nil most-positive-fixnum t)
      (set-window-new-total nil most-positive-fixnum t)
      (set-window-new-total nil most-positive-fixnum t)
      (set-window-new-total nil most-positive-fixnum t)
      (window-new-total nil)
      (set-window-new-total nil most-negative-fixnum)
      (set-window-new-total nil -1 t)
      (window-new-total nil))"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-window-total.expect").trim_end()
    );
}

#[test]
fn tsb_split_checks_window_and_size_before_minibuffer_geometry() {
    let form = r#"(let ((before (list (selected-window) (window-list) (window-edges)
                                    (current-buffer) (point) (window-point) (window-start)
                                    (window-new-pixel) (window-new-total) (window-new-normal))))
      (list
        (condition-case e (split-window-internal 'bogus 'bad-size nil nil) (error e))
        (condition-case e (split-window-internal (minibuffer-window) 'bad-size nil nil) (error e))
        (condition-case e (split-window-internal (minibuffer-window) 0 nil nil) (error e))
        (condition-case e (split-window-internal (minibuffer-window) most-positive-fixnum nil nil) (error e))
        (equal before (list (selected-window) (window-list) (window-edges)
                            (current-buffer) (point) (window-point) (window-start)
                            (window-new-pixel) (window-new-total) (window-new-normal)))))"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-window-split-precedence.expect").trim_end()
    );
}

#[test]
fn tsb_split_requires_a_fitting_staged_old_extent_before_tree_mutation() {
    let form = r#"(let* ((old (selected-window))
                        (whole (window-pixel-height old))
                        (half (/ whole 2))
                        (before (list (window-list) (window-edges) (point)
                                      (window-point) (window-start))))
      (list
        (condition-case e (split-window-internal old half nil nil) (error e))
        (progn (set-window-new-pixel old 1)
          (condition-case e (split-window-internal old half nil nil) (error e)))
        (equal before (list (window-list) (window-edges) (point)
                            (window-point) (window-start)))
        (progn (set-window-new-pixel old (- whole half))
          (window-live-p (split-window-internal old half nil 0.5)))
        (length (window-list))))"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-window-split-staging.expect").trim_end()
    );
}

#[test]
fn tsb_window_pixel_addition_preserves_gnu_failed_plan_staging() {
    let form = r#"(let* ((window-combination-resize nil)
                        (old (selected-window))
                        (sibling (split-window old nil 'below))
                        (parent (window-parent old))
                        (physical (window-pixel-height parent))
                        (before (list (window-list) (window-pixel-edges old)
                                      (window-pixel-edges sibling) (window-pixel-edges parent)))
                        (window-combination-resize t))
      (list
        (condition-case e (split-window-internal old most-positive-fixnum nil 0.5) (error e))
        (= (window-new-pixel parent) (- physical most-positive-fixnum))
        (= (set-window-new-pixel parent (- (window-new-pixel parent)) t) most-negative-fixnum)
        (= (window-new-pixel parent) most-negative-fixnum)
        (condition-case e (split-window-internal parent 1 nil 0.5) (error e))
        (equal before (list (window-list) (window-pixel-edges old)
                            (window-pixel-edges sibling) (window-pixel-edges parent)))))"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-window-pixel-staging.expect").trim_end()
    );
}
