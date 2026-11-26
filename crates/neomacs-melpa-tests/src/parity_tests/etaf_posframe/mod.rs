//! Behavioural parity for the pinned `etaf-posframe` headless surface.
//!
//! Panel creation needs a graphical child frame and is covered by the GUI
//! corpus.  These probes cover the parts that run in batch: option validation
//! before a frame exists, palette derivation from the current batch frame,
//! empty-panel queries, and operations on values that are not panels.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-posframe)

(defun neomacs-posframe-error (thunk)
  "Return THUNK's error as (TYPE MESSAGE)."
  (condition-case err
      (funcall thunk)
    (error (list (car err) (cadr err)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-posframe", "0.3.0"), "etaf-posframe.el")
        .expect("prepare pinned etaf-posframe source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_posframe_behavior_batch() {
    let cases = vec![
        // Every invalid panel option is rejected with a message naming the
        // option before any child frame is created.
        ParityBatchCase::value(
            "invalid_options",
            r####"
(let ((view (etaf-view (text "panel"))))
  (list
   (neomacs-posframe-error (lambda () (etaf-posframe-show view :bogus 1)))
   (neomacs-posframe-error (lambda () (etaf-posframe-show view :width 0)))
   (neomacs-posframe-error
    (lambda () (etaf-posframe-show view :auto-hide 'never)))
   (neomacs-posframe-error (lambda () (etaf-posframe-show view :at 'nowhere)))
   (neomacs-posframe-error (lambda () (etaf-posframe-show view :theme 'sepia)))
   (neomacs-posframe-error
    (lambda () (etaf-posframe-show view :resizable 'sideways)))
   (neomacs-posframe-error
    (lambda () (etaf-posframe-show view :resizable '(left middle))))))
"####,
            expect![[
                r#"OK ((etaf-posframe-error "Unknown panel option: :bogus") (etaf-posframe-error "Panel :width must be a positive cells count or (fraction F): 0") (etaf-posframe-error "Panel :auto-hide must be t, nil or positive seconds: never") (etaf-posframe-error "Unsupported panel :at: nowhere") (etaf-posframe-error "Panel :theme must be auto, light, dark, a plist, ref or computed") (etaf-posframe-error "Panel :resizable must be t, nil or a list of (left right top bottom): sideways") (etaf-posframe-error "Panel :resizable must be t, nil or a list of (left right top bottom): (left middle)"))"#
            ]],
        ),
        // The Theme plist is derived from the batch frame's colors; it is the
        // same basic-color environment in both editors and is not a claim
        // about a graphical palette.
        ParityBatchCase::value(
            "theme_from_batch_frame",
            r####"
(list (etaf-posframe-theme)
      (etaf-posframe-current-theme))
"####,
            expect![[
                r##"OK ((:ui-mode dark :ui-bg "#16191D" :ui-fg "#D8DEE6" :ui-muted-fg "#9393ff" :ui-border "#2828ff" :ui-panel-bg "#0000ff" :ui-panel-fg "#D8DEE6" :ui-panel-border "#2828ff" :ui-accent "#6CB6FF" :ui-accent-fg "#101418" :ui-accent-soft-bg "#001eff" :ui-disabled-fg "#6666ff" :ui-disabled-bg "#0000ff" :ui-disabled-border "#2828ff" :ui-button-primary-fg "#101418" :ui-button-primary-bg "#6CB6FF" :ui-button-secondary-fg "#a5ffff" :ui-button-secondary-bg "#002dff" :ui-button-ghost-fg "#D8DEE6" :ui-info-bg "#0033ff" :ui-info-fg "#72ffff" :ui-info-border "#0066ff" :ui-success-bg "#0033cc" :ui-success-fg "#7fff7f" :ui-success-border "#006699" :ui-warning-bg "#3333cc" :ui-warning-fg "#ffff72" :ui-warning-border "#666699" :ui-danger-bg "#3300cc" :ui-danger-fg "#ff7272" :ui-danger-border "#660099") nil)"##
            ]],
        ),
        // No panel is live before a graphical show, and operations that need
        // one report their documented errors instead of creating a panel.
        ParityBatchCase::value(
            "non_panel_operations",
            r####"
(list (etaf-posframe-live-p nil)
      (etaf-posframe-live-p 'not-a-panel)
      (etaf-posframe-panels)
      (etaf-posframe-hide nil)
      (neomacs-posframe-error (lambda () (etaf-posframe-hide 'not-a-panel)))
      (neomacs-posframe-error
       (lambda () (etaf-posframe-move 'not-a-panel 1 2)))
      (neomacs-posframe-error
       (lambda () (etaf-posframe-resize 'not-a-panel 10 4)))
      (etaf-posframe-refresh 'not-a-panel))
"####,
            expect![[
                r#"OK (nil nil nil nil (wrong-type-argument etaf-posframe--panel) (etaf-posframe-error "No live ETAF panel to move") (etaf-posframe-error "No live ETAF panel to resize") nil)"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-posframe-behavior", "etaf-posframe", &cases);
}
