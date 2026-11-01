//! GNU `frame-or-buffer-changed-p` state vector semantics.
//!
//! GNU implements this in `src/dispnew.c:7049`: the symbol's value is a vector
//! holding every frame and its name, then every live buffer whose name does
//! not start with a space together with its read-only and modified state,
//! terminated by `lambda`. A call compares the vector with the current state;
//! on a change it rewrites (or reallocates) the vector and returns t.
//!
//! The snapshot below is the GNU 31.1 batch answer. It drives one user
//! variable through buffer creation, modification, read-only changes,
//! hidden-buffer exclusion, deletion, non-vector replacement, the internal
//! state used by a nil VARIABLE, and the error boundaries.

use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn frame_or_buffer_changed_p_tracks_gnu_state_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(let ((var 'neb-frame-buffer-state) visible hidden)
  (unwind-protect
      (progn
        (set var nil)
        (let (results)
          (push (frame-or-buffer-changed-p var) results)
          (push (frame-or-buffer-changed-p var) results)
          (setq visible (generate-new-buffer "neb-visible"))
          (push (frame-or-buffer-changed-p var) results)
          (push (frame-or-buffer-changed-p var) results)
          (with-current-buffer visible (insert "x"))
          (push (frame-or-buffer-changed-p var) results)
          (push (frame-or-buffer-changed-p var) results)
          (with-current-buffer visible (setq buffer-read-only t))
          (push (frame-or-buffer-changed-p var) results)
          (push (frame-or-buffer-changed-p var) results)
          (setq hidden (generate-new-buffer " *neb-hidden*"))
          (push (frame-or-buffer-changed-p var) results)
          (with-current-buffer hidden (insert "h"))
          (push (frame-or-buffer-changed-p var) results)
          (kill-buffer visible)
          (setq visible nil)
          (push (frame-or-buffer-changed-p var) results)
          (push (frame-or-buffer-changed-p var) results)
          (set var 5)
          (push (frame-or-buffer-changed-p var) results)
          (push (frame-or-buffer-changed-p var) results)
          (frame-or-buffer-changed-p)
          (push (frame-or-buffer-changed-p) results)
          (push (frame-or-buffer-changed-p) results)
          (push (frame-or-buffer-changed-p nil) results)
          (push (condition-case e (frame-or-buffer-changed-p 'neb-unbound)
                  (error (car e)))
                results)
          (push (condition-case e (frame-or-buffer-changed-p 1)
                  (error (car e)))
                results)
          (nreverse results)))
    (when (buffer-live-p visible) (kill-buffer visible))
    (when (buffer-live-p hidden) (kill-buffer hidden))))"##;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq(
        "(t nil t nil t nil t nil nil nil t nil t nil nil nil nil void-variable wrong-type-argument)",
        &gnu,
        &neomacs,
    );
}

/// GNU sizes the fresh vector from every live buffer with no name filter:
/// `FOR_EACH_LIVE_BUFFER n += 3` (src/dispnew.c:7116-7122). The leading-space
/// filter applies only when entries are compared and recorded, so adding a
/// hidden buffer still grows the reallocated vector by exactly three slots.
#[test]
fn hidden_buffers_are_counted_in_the_state_vector_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(let ((var 'neb-hidden-size-state) hidden delta)
  (unwind-protect
      (progn
        (set var nil)
        ;; A far-oversized vector forces GNU's reallocation branch, whose
        ;; fresh length is n + 20 for the current live-buffer count.
        (set var (make-vector 4000 'lambda))
        (frame-or-buffer-changed-p var)
        (let ((before (length (symbol-value var))))
          (setq hidden (generate-new-buffer " *neb-size-hidden*"))
          (set var (make-vector 4000 'lambda))
          (frame-or-buffer-changed-p var)
          (setq delta (- (length (symbol-value var)) before)))
        delta)
    (when (buffer-live-p hidden) (kill-buffer hidden))))"##;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq("3", &gnu, &neomacs);
}

#[test]
fn ibuffer_auto_mode_initializes_its_state_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // ibuf-ext.el:402 initializes the state vector by calling
    // frame-or-buffer-changed-p with ibuffer-auto-buffers-changed.
    let form = r##"(progn
  (require 'ibuffer)
  (with-temp-buffer
    (ibuffer-mode)
    (ibuffer-auto-mode 1)
    (list ibuffer-auto-mode
          (boundp 'ibuffer-auto-buffers-changed)
          (vectorp ibuffer-auto-buffers-changed))))"##;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq("(t t t)", &gnu, &neomacs);
}
