//! Behavioural parity for the pinned `etaf-screensaver` public surface.
//!
//! The MELPA build installs no `scenes/` or `reels/` resource directories, so
//! the bundled registry is the two Lisp scenes (`donut`, `equalizer`).  These
//! probes drive scene lookup, registration and replacement, directory loading
//! with files written by the probe, the start/stop lifecycle, and input
//! dismissal.  Animated frame content depends on elapsed time and is not
//! pinned; the assertions read the public status plist instead.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-screensaver)

(defun neomacs-screensaver-error (thunk)
  "Return THUNK's error as (TYPE MESSAGE)."
  (condition-case err
      (funcall thunk)
    (error (list (car err) (cadr err)))))

(defun neomacs-screensaver-spec (id title line)
  "Return a minimal text-scene spec named ID with TITLE and LINE."
  (list :id id :title title :summary "Probe scene."
        :tags '(probe) :frame-rate 10
        :layers (list (list :kind 'text :color "#FFFFFF"
                            :lines (list line)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-screensaver", "0.1.1"), "etaf-screensaver.el")
        .expect("prepare pinned etaf-screensaver source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_screensaver_behavior_batch() {
    let cases = vec![
        // The bundled registry exposes the two Lisp scenes by id, symbol name
        // and displayed title.
        ParityBatchCase::value(
            "scene_registry",
            r####"
(list (cl-subseq (etaf-screensaver-scenes) 0 2)
      (etaf-screensaver-scene-info 'donut)
      (etaf-screensaver-scene-info "donut")
      (etaf-screensaver-scene-info "旋转甜甜圈")
      (etaf-screensaver-scene-info 'equalizer)
      (neomacs-screensaver-error
       (lambda () (etaf-screensaver-scene-info 'nope))))
"####,
            expect![[
                r#"OK ((donut equalizer) (:id donut :title "旋转甜甜圈" :summary "经典 ASCII 甜甜圈：逐格 z-buffer 上色，明暗当墨。" :tags #1=(3d math monochrome) :frame-rate 15 :params #2=((:key speed :label "旋转速度" :type number :min 1 :max 20 :step 1 :default 6) (:key detail :label "采样精度" :type choice :options ((coarse . "低") (normal . "中") (fine . "高")) :default normal))) (:id donut :title "旋转甜甜圈" :summary "经典 ASCII 甜甜圈：逐格 z-buffer 上色，明暗当墨。" :tags #1# :frame-rate 15 :params #2#) (:id donut :title "旋转甜甜圈" :summary "经典 ASCII 甜甜圈：逐格 z-buffer 上色，明暗当墨。" :tags #1# :frame-rate 15 :params #2#) (:id equalizer :title "墨线律动" :summary "单色竖柱随节拍起伏" :tags (bars ink) :frame-rate 24 :params nil) (etaf-screensaver-error "Unknown scene: nope"))"#
            ]],
        ),
        // Registering an id appends it; registering it again replaces the
        // scene in place, and a scene without layers is rejected.
        ParityBatchCase::value(
            "custom_scene_registration",
            r####"
(progn
  (etaf-screensaver-register-scene
   (neomacs-screensaver-spec 'neomacs-registered "Registered" "registered line"))
  (let ((added (list (nth 2 (etaf-screensaver-scenes))
                     (etaf-screensaver-scene-info 'neomacs-registered))))
    (etaf-screensaver-register-scene
     (neomacs-screensaver-spec 'neomacs-registered "Registered 2" "replaced line"))
    (list added
          (nth 2 (etaf-screensaver-scenes))
          (plist-get (etaf-screensaver-scene-info 'neomacs-registered) :title)
          (length (etaf-screensaver-scenes))
          (neomacs-screensaver-error
           (lambda () (etaf-screensaver-register-scene '(:id broken)))))))
"####,
            expect![[
                r#"OK ((neomacs-registered (:id neomacs-registered :title "Registered" :summary "Probe scene." :tags (probe) :frame-rate 10 :params nil)) neomacs-registered "Registered 2" 3 (etaf-screensaver-error "Scene broken needs at least one :layer"))"#
            ]],
        ),
        // Loading a directory registers valid .esc files, skips a broken file
        // and ignores other extensions; a missing directory loads zero.
        ParityBatchCase::value(
            "load_scenes_directory",
            r####"
(let* ((directory (make-temp-file "neomacs-screensaver-" t))
       (spec (neomacs-screensaver-spec 'neomacs-loaded-scene "Loaded"
                                       "loaded line"))
       (valid (expand-file-name "probe.esc" directory))
       (broken (expand-file-name "broken.esc" directory))
       (ignored (expand-file-name "ignore.txt" directory)))
  (with-temp-file valid
    (prin1 (cons 'etaf-scene spec) (current-buffer))
    (insert "\n"))
  (with-temp-file broken
    (insert "(etaf-scene :id broken\n"))
  (with-temp-file ignored
    (insert "not a scene\n"))
  (let ((loaded (etaf-screensaver-load-scenes (list directory))))
    (list loaded
          (memq 'neomacs-loaded-scene (etaf-screensaver-scenes))
          (etaf-screensaver-scene-info 'neomacs-loaded-scene)
          (etaf-screensaver-load-scenes
           (list (expand-file-name "neomacs-screensaver-missing" directory))))))
"####,
            expect![[
                r#"OK (1 (neomacs-loaded-scene) (:id neomacs-loaded-scene :title "Loaded" :summary "Probe scene." :tags (probe) :frame-rate 10 :params nil) 0)"#
            ]],
        ),
        // Starting presents the requested scene, a second start keeps the
        // session, and stopping is idempotent and releases the buffer.
        ParityBatchCase::value(
            "start_stop_lifecycle",
            r####"
(let ((etaf-screensaver-seed 4242))
  (unwind-protect
      (let ((before (etaf-screensaver-status))
            (buffer (etaf-screensaver-start 'donut))
            (again (etaf-screensaver-start 'equalizer)))
        (let ((status (etaf-screensaver-status)))
          (let ((started (list (eq buffer again)
                               (buffer-name buffer)
                               (plist-get status :active)
                               (plist-get status :scene)
                               (plist-get status :index)
                               (plist-get status :seed))))
            (etaf-screensaver-stop)
            (list before started
                  (buffer-live-p buffer)
                  (etaf-screensaver-status)
                  (etaf-screensaver-stop)
                  (neomacs-screensaver-error
                   (lambda () (etaf-screensaver-start 'nope)))
                  (etaf-screensaver-status)))))
    (etaf-screensaver-stop)))
"####,
            expect![[
                r#"OK ((:active nil :automatic nil) (t " *ETAF Screensaver*" t donut 0 4242) nil (:active nil :automatic nil) nil (etaf-screensaver-error "Unknown scene: nope") (:active nil :automatic nil))"#
            ]],
        ),
        // The first key is consumed by the presentation: it dismisses the
        // screensaver, restores the original buffer, and leaves text and point
        // untouched.
        ParityBatchCase::value(
            "key_wakes_without_editing",
            r####"
(save-window-excursion
  (with-temp-buffer
    (insert "original content")
    (switch-to-buffer (current-buffer))
    (let ((original (current-buffer))
          (position (point))
          (etaf-screensaver-seed 4242))
      (unwind-protect
          (progn
            (etaf-screensaver-start 'donut)
            (let ((active (plist-get (etaf-screensaver-status) :active)))
              (execute-kbd-macro (kbd "x"))
              (list active
                    (plist-get (etaf-screensaver-status) :active)
                    (eq (window-buffer) original)
                    (buffer-substring-no-properties (point-min) (point-max))
                    (= (point) position))))
        (etaf-screensaver-stop)))))
"####,
            expect![[r#"OK (t nil t "original content" t)"#]],
        ),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "etaf-screensaver-behavior",
        "etaf-screensaver",
        &cases,
    );
}
