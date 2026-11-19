//! Real graphical parity for the pinned `etaf-algo-lab` workspace.
//!
//! The sorting laboratory is mounted at its initial paused state and its
//! painted text and pixel size are compared in a real X11 frame.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (require 'etaf-algo-lab)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)

  (let ((buffer (etaf-algo-lab-open 'sorting " *etaf-algo-lab-parity*")))
    (unwind-protect
        (progn
          (set-window-buffer (selected-window) buffer)
          (with-current-buffer buffer
            (redisplay t)
            (let ((window (get-buffer-window buffer t)))
              (list :labs (etaf-algo-lab-labs)
                    :text (buffer-substring-no-properties (point-min) (point-max))
                    :pixel-size (and window (window-text-pixel-size
                                             window (point-min) (point-max)))
                    :body-width (and window (window-body-width window))
                    :body-height (and window (window-body-height window))
                    :default-family (face-attribute 'default :family nil t)
                    :default-height (face-attribute 'default :height nil t)
                    :frame-width (frame-parameter nil 'width)
                    :frame-height (frame-parameter nil 'height)))))
      (ignore-errors (etaf-algo-lab-close (buffer-name buffer)))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"####;

// Known crash, surfaced 2026-11-19: mounting either laboratory aborts Neomacs
// with an allocation failure while GNU renders it.  The core dump shows
// unbounded recursion in neovm_value TaggedValue::to_equal_key_depth_swp
// (equal hash keys of a shared structure); both laboratories abort at 4, 8 and
// 32 GB address-space limits, so this is unbounded work, not a memory budget.
// The test stays as the reproducer; run it with --run-ignored once fixed.
#[test]
#[ignore = "Neomacs OOM-aborts mounting any etaf-algo-lab workspace (to_equal_key_depth_swp); GNU renders it"]
fn etaf_algo_lab_real_gui_workspace_matches_gnu() {
    let oracle = CachedMelpaOracle::new(("etaf-algo-lab", "0.1.1"), "etaf-algo-lab.el")
        .expect("prepare pinned etaf-algo-lab source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "etaf-algo-lab-real-gui-workspace",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    let expected_gnu = expect![[r#""""#]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    let expected_neomacs = expect![[r#""""#]];
    expected_neomacs.assert_eq(&pair.neomacs_behavior.to_string());
    assert_ne!(
        pair.neomacs_behavior, pair.gnu_behavior,
        "the pinned etaf-algo-lab GUI divergences are FIXED: flip this test to \
         assert_eq and re-record both snapshots; full outcomes:\nNeomacs: {}\nGNU Emacs: {}",
        pair.neomacs, pair.gnu_emacs,
    );
}
