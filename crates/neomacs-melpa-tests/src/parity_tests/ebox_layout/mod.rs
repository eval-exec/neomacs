//! Behavioural parity for the pinned `ebox` retained layout engine.
//!
//! The probes use the package's public author DSL and `ch`/`lh` units, whose
//! geometry is expressed in character cells and therefore does not depend on
//! the host font's pixel metrics.  They cover the renderer, mounted-tree
//! mutation and the documented error contract without requiring a GUI frame.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-ebox-in-buffer (input body)
  "Mount INPUT in a fresh buffer and call BODY with (ROOT BUFFER)."
  (let ((buffer (generate-new-buffer " *ebox-parity*")))
    (unwind-protect
        (let ((root (ebox-mount buffer input)))
          (with-current-buffer buffer
            (funcall body root buffer)))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-ebox-text (input)
  "Render INPUT and return its text without properties."
  (substring-no-properties (ebox-render input)))
"####;

fn ebox_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("ebox", "3.1.2"), "ebox.el")
        .expect("prepare pinned ebox source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn ebox_layout_batch() {
    let cases = vec![
        // Row flow with item gaps and fixed box widths.
        ParityBatchCase::value(
            "render_row_and_column_flow",
            r####"
(list (neomacs-ebox-text
       '(row :item-gap (ch 2)
             (box :width (ch 3) "A")
             (box :width (ch 3) "B")))
      (neomacs-ebox-text
       '(column :width (ch 3)
                (box :height (lh 1) "A")
                (box :height (lh 1) "B"))))
"####,
            expect![[r#"OK ("A  B  " "A \nB ")"#]],
        ),
        // Wrapping, text alignment and CJK measurement.
        ParityBatchCase::value(
            "render_wrap_align_cjk",
            r####"
(list (neomacs-ebox-text '(box :width (ch 3) "abcdef"))
      (neomacs-ebox-text '(box :width (ch 4) :text-align right "Hi"))
      (neomacs-ebox-text '(box :width (ch 4) :text-align center "Hi"))
      (neomacs-ebox-text '(box :width (ch 4) "你好吗"))
      (neomacs-ebox-text '(box :width (ch 4) "你好。好")))
"####,
            expect![[r#"OK ("abc\ndef" " Hi" " Hi " "你好\n吗 " "你 \n好。\n好 ")"#]],
        ),
        // Author DSL and explicit node construction build equivalent trees.
        ParityBatchCase::value(
            "build_dsl_and_nodes",
            r####"
(list (neomacs-ebox-text
       (ebox-build '(column (text "x") (row (box "y") (box "z")))))
      (neomacs-ebox-text
       (ebox-node-create
        'column nil
        (list (ebox-text-create "x")
              (ebox-node-create
               'row nil
               (list (ebox-node-create 'box nil (list (ebox-text-create "y")))
                     (ebox-node-create 'box nil (list (ebox-text-create "z"))))))))
      (ebox-node-child-count
       (ebox-build '(column (text "x") (row (box "y") (box "z")))))
      (ebox-node-text-p (ebox-text-create "x")))
"####,
            expect![[r#"OK ("x \nyz " "x \nyz " 2 t)"#]],
        ),
        // Flex wrapping geometry, read from the mounted tree.
        ParityBatchCase::value(
            "flex_wrap_geometry",
            r####"
(neomacs-ebox-in-buffer
 '(flex :width (ch 8) :flex-flow (row wrap)
        :column-gap (ch 1) :row-gap (lh 1)
        (box :id "a" :width (ch 3) "A")
        (box :id "b" :width (ch 3) "B")
        (box :id "c" :width (ch 3) "C"))
 (lambda (root buffer)
   (let ((a (car (ebox-selector-query root "#a")))
         (b (car (ebox-selector-query root "#b")))
         (c (car (ebox-selector-query root "#c"))))
     (list (ebox-node-geometry root)
           (ebox-node-geometry a)
           (ebox-node-geometry b)
           (ebox-node-geometry c)
           (ebox-node-index a)
           (ebox-node-index c)
           (ebox-node-ancestor-p root c)))))
"####,
            expect![
                "OK ((:x 0 :y 0 :width 8 :height 3) (:x 0 :y 0 :width 3 :height 1) (:x 4 :y 0 :width 3 :height 1) (:x 0 :y 2 :width 3 :height 1) 0 2 t)"
            ],
        ),
        // Tree mutation through render output: insert, move and deep copy.
        ParityBatchCase::value(
            "tree_mutation",
            r####"
(let* ((root (ebox-build '(row (text "A") (text "B") (text "C"))))
       (before (neomacs-ebox-text root)))
  (ebox-node-insert root (ebox-text-create "D") 1)
  (let ((inserted (neomacs-ebox-text root)))
    (ebox-node-move (ebox-node-child root 0) 2)
    (let ((moved (neomacs-ebox-text root))
          (copy (neomacs-ebox-text (ebox-node-copy root))))
      (ebox-node-set-text (ebox-node-child root 0) "changed")
      (list before inserted moved copy
            (neomacs-ebox-text root)
            (eq root (ebox-node-copy root))))))
"####,
            expect![[r#"OK ("ABC " "ADBC " "DBAC " "DBAC " "changedBAC " nil)"#]],
        ),
        // A mounted flush publishes mutations and records its write count.
        ParityBatchCase::value(
            "mounted_flush",
            r####"
(neomacs-ebox-in-buffer
 '(column (text "one") (text "two"))
 (lambda (root buffer)
   (let ((before (buffer-substring-no-properties (point-min) (point-max)))
         (flushes-before (ebox-mount-flush-count buffer)))
     (ebox-with-batch
       (ebox-node-set-text (ebox-node-child root 0) "ONE"))
     (list before
           (buffer-substring-no-properties (point-min) (point-max))
           flushes-before
           (ebox-mount-flush-count buffer)
           (eq (ebox-buffer-root buffer) root)))))
"####,
            expect![[r#"OK ("one \ntwo " "ONE \ntwo " 1 2 t)"#]],
        ),
        // The documented invalid-property contract signals `ebox-error`.
        ParityBatchCase::value(
            "invalid_property_signal",
            r####"
(list (condition-case err
          (ebox-render
           '(box :border-top ((px 2) solid "#00ff00") "Hi"))
        (error (list (car err) (cdr err))))
      (condition-case err
          (ebox-build '(text :unknown-property t "x"))
        (error (list (car err) (cdr err)))))
"####,
            expect![[
                r#"OK ((ebox-error ("Ebox top and bottom borders support only 1px width" "top" (px 2))) (ebox-error ("Ebox text rejected properties: Unknown Ebox style property: :unknown-property")))"#
            ]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("ebox parity batch");
    assert_oracle_batch_cases(ebox_oracle(), test_name, "ebox parity", &cases);
}
