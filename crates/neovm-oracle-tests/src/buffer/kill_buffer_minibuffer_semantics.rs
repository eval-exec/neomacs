//! GNU `kill-buffer` refuses to kill the buffer displayed in `minibuf_window`.
//!
//! GNU `Fkill_buffer` checks after the query functions and `kill-buffer-hook`
//! have run: a buffer whose contents are those of `minibuf_window` is not
//! killed, and the function returns nil (src/buffer.c:1998-2000, "Don't kill
//! the minibuffer now current"). Killing it would leave an ordinary buffer
//! displayed in the minibuffer window. Neomacs previously killed it and the
//! dead-window fallback put `*scratch*` there (issue #656).
//!
//! The control case pins that the guard is narrow: an ordinary buffer still
//! dies and reports t.

use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn kill_buffer_refuses_the_buffer_displayed_in_the_minibuffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(let* ((mini (minibuffer-window))
       (original (window-buffer mini))
       (victim (generate-new-buffer "neb-kill-mini-victim"))
       (ordinary (generate-new-buffer "neb-kill-mini-ordinary")))
  (unwind-protect
      (progn
        (set-window-buffer mini victim)
        (list (kill-buffer victim)
              (buffer-live-p victim)
              (buffer-name (window-buffer mini))
              (kill-buffer ordinary)
              (buffer-live-p ordinary)))
    (when (buffer-live-p victim)
      (set-window-buffer mini original)
      (kill-buffer victim))))"##;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq("(nil t \"neb-kill-mini-victim\" t nil)", &gnu, &neomacs);
}
