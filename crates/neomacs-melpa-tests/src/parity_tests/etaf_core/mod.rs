//! Behavioural parity for the pinned `etaf` framework facade.
//!
//! Probes cover the deterministic layers that do not need a live Host: reactive
//! refs and watchers, the memory Data source and controller, animation
//! interpolation, and View normalization/lowering/mounting.  App lifecycle,
//! process, animation clocks and host interaction stay in the GUI/interaction
//! lanes.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)

(defconst neomacs-etaf-data-records
  '((:id 1 :name "Ada" :group "compiler")
    (:id 2 :name "Grace" :group "systems")
    (:id 3 :name "Alan" :group "compiler")
    (:id 4 :name "Barbara" :group "compiler")))

(defun neomacs-etaf-data-ids (items)
  "Return :id values from ITEMS, tolerating plists and alists."
  (mapcar (lambda (item)
            (cond
             ((hash-table-p item) (gethash :id item))
             ((and (proper-list-p item) (keywordp (car item)))
              (plist-get item :id))
             ((listp item) (alist-get :id item nil nil #'equal))))
          items))
"####;

fn etaf_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf", "0.3.5"), "etaf.el")
        .expect("prepare pinned etaf source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_core_batch() {
    let cases = vec![
        // Reactive refs publish the new and old value to watchers, and an
        // unwatched cell stops publishing.
        ParityBatchCase::value(
            "reactive_refs_and_watchers",
            r####"
(let* ((ref (etaf-ref 1 :name 'counter))
       (events nil)
       (stop (etaf-watch ref (lambda (new old) (push (list new old) events)))))
  (setf (etaf-value ref) 2)
  (setf (etaf-value ref) 3)
  (funcall stop)
  (setf (etaf-value ref) 4)
  (list (etaf-value ref)
        (nreverse events)
        (etaf-ref-p ref)
        (etaf-ref-p 4)))
"####,
            expect!["OK (4 ((2 1) (3 2)) t nil)"],
        ),
        // The memory Data source's paging and every public query form.
        ParityBatchCase::value(
            "memory_source_paging",
            r####"
(let* ((hash (make-hash-table :test #'equal))
       (records (append neomacs-etaf-data-records
                        (list '((:id . 5) (:name . "Edsger") (:group . "systems"))
                              hash)))
       (source (etaf-data-memory-source
                records :id-key :id :name 'memory)))
  (puthash :id 6 hash)
  (puthash :name "Frances" hash)
  (puthash :group "compiler" hash)
  (list (plist-get (etaf-data-source-capabilities source) :provider)
        (neomacs-etaf-data-ids
         (plist-get (etaf-data-source-load-page source nil 1 2) :items))
        (neomacs-etaf-data-ids
         (plist-get (etaf-data-source-load-page source nil 2 2) :items))
        (plist-get (etaf-data-source-load-page
                    source '(:group "compiler") 1 10) :total)
        (neomacs-etaf-data-ids
         (plist-get (etaf-data-source-load-page
                     source '((:group . "systems")) 1 10) :items))
        (neomacs-etaf-data-ids
         (plist-get (etaf-data-source-load-page
                     source (lambda (row)
                              (equal (cond
                                      ((hash-table-p row) (gethash :group row))
                                      ((and (proper-list-p row) (keywordp (car row)))
                                       (plist-get row :group))
                                      ((listp row) (alist-get :group row nil nil #'equal)))
                                     "compiler"))
                     1 10) :items))
        (neomacs-etaf-data-ids
         (plist-get (etaf-data-source-load-page source "Grace" 1 10) :items))))
"####,
            expect!["OK (memory (1 2) (3 4) 4 (2 5) (1 3 4 6) (2))"],
        ),
        // A controller accepts an initial result and exposes its status and
        // item refs before any load runs.
        ParityBatchCase::value(
            "data_controller_initial",
            r####"
(let* ((source (etaf-data-memory-source
                neomacs-etaf-data-records :id-key :id))
       (controller (etaf-data-controller
                    source :initial-result '(:items ((:id 1)) :total 1))))
  (unwind-protect
      (list (etaf-value (etaf-data-status controller))
            (neomacs-etaf-data-ids (etaf-value (etaf-data-items controller))))
    (etaf-data-stop controller)))
"####,
            expect!["OK (success (1))"],
        ),
        // Animation interpolation for numbers, colors and lh units.
        ParityBatchCase::value(
            "animation_interpolation",
            r####"
(list (etaf-animation-interpolate 0 10 0.5)
      (etaf-animation-interpolate 0 10 1)
      (etaf-animation-interpolate 3 9 0)
      (etaf-animation-interpolate "#000000" "#ffffff" 0.5)
      (etaf-animation-interpolate "#000000" "#123456" 1)
      (etaf-animation-interpolate '(lh 1) '(lh 4) 0.5))
"####,
            expect![[r##"OK (5.0 10 3 "#7fff7fff7fff" "#123456" (lh 2.5))"##]],
        ),
        // View normalization and lowering to a canonical Ebox input.
        ParityBatchCase::value(
            "view_normalize_and_render",
            r####"
(list (etaf-view (text "hello"))
      (etaf-render (etaf-view (text "hello")))
      (etaf-view (column (text "a") (text "b")))
      (etaf-node 'text '(:class "x") (list "payload")))
"####,
            expect![[
                r#"OK (#s(etaf--view-node text (etaf-compiled-site "1e65c93207b25f0f464d8278ca8d68cd2bfb55b034f88007b0d2d2bdfc5fa9d8" (0)) nil (#1="hello")) #s(ebox-node text nil nil nil 0 nil 0 #1# nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil 15 0 0 0 0 nil nil nil nil nil nil nil nil nil nil nil nil 0 t nil nil nil) #s(etaf--view-node column (etaf-compiled-site #2="d77b194c31f67641d6f0ba8c822158082077dcf3eea502d7a0ddaa56c2404f17" (0)) nil (#s(etaf--view-node text (etaf-compiled-site #2# (0 0)) nil ("a")) #s(etaf--view-node text (etaf-compiled-site #2# (0 1)) nil ("b")))) #s(etaf--view-node text nil (:class "x") ("payload")))"#
            ]],
        ),
        // Mounting a View produces the same buffer text a user sees.
        ParityBatchCase::value(
            "view_mount_buffer_text",
            r####"
(let ((buffer (generate-new-buffer " *etaf-parity*")))
  (unwind-protect
      (progn
        (etaf-mount buffer (etaf-view (column (text "one") (text "two"))))
        (with-current-buffer buffer
          (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"####,
            expect![[r#"OK "one \ntwo ""#]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf parity batch");
    assert_oracle_batch_cases(etaf_oracle(), test_name, "etaf parity", &cases);
}
