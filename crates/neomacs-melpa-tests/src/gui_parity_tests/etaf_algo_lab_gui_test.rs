//! Real graphical parity for the pinned `etaf-algo-lab` workspace.
//!
//! The sorting laboratory is mounted at its initial paused state and its
//! painted text and pixel size are compared in a real X11 frame.  The probe
//! asserts strict GNU parity; Neomacs must produce GNU's exact observation.

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

// Regression test for the 2026-11-19 crash: mounting either laboratory used
// to abort Neomacs with an allocation failure while GNU renders it.  The core
// dump showed unbounded recursion in neovm_value
// TaggedValue::to_equal_key_depth_swp (the stored equal key of a shared
// structure); both laboratories aborted at 4, 8 and 32 GB address-space
// limits, so it was unbounded work, not a memory budget.  The stored key walk
// is now node-budgeted.  This strict parity test reports the remaining
// divergences as failures: the U+200B line prefix, a 100 vs 101 default
// height, and 594 vs 581 pixels for the same 34 body lines.
#[test]
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

    let expected_gnu = expect![[
        r#"OK (:labs (pathfinding sorting) :text " \n 算法实验室  寻路实验台   排序实验台   \n   \n 排序实验台 准备就绪  \n   \n 每一列的高度就是一个数字，最下面一行是它的值。  \n 黄色 = 正在比较，紫色 = 正在写回，绿色 = 已经排好。  \n   \n   \n   开始/继续   暂停   单步   跳到完成    \n     \n   重置   批量对比    \n     \n  速度  慢速   正常   快速    \n     \n  算法  冒泡   插入   选择   归并    \n   \n   \n   \n  比较 0 次 · 写入 0 次   \n     \n  跑完后这里记住每种算法比较了多少次。   \n   \n   \n    \n                                                                                      \n                                                                                      \n                                                                                      \n                                                                                      \n                                                                                      \n                                                                                      \n                                                                                      \n                                                                                      \n                                                                                      " :pixel-size (800 . 581) :body-width 100 :body-height 34 :default-family "DejaVu Sans Mono" :default-height 101 :frame-width 100 :frame-height 35)"#
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    // Strict parity: the probe must produce GNU's exact observation.  There
    // is deliberately no pinned Neomacs outcome, so a divergence cannot be
    // blessed by re-recording it.
    assert_eq!(
        pair.neomacs_behavior.to_string(),
        pair.gnu_behavior.to_string(),
        "Neomacs must match GNU exactly; open ETAF GUI divergences are bugs to \
         fix, not values to re-pin"
    );
}
