use std::time::Duration;

use crate::{ASM_BLOX_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod files;
mod parser;
mod puzzles;
mod registry;
mod runtime;
mod sources_sinks;
mod workflows;
mod yaml_cells;

const ASM_BLOX_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASM_BLOX_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(setq max-lisp-eval-depth 10000)

(defvar asm-blox-test-random-values nil)

(defun asm-blox-test-random
    (limit)
  (let ((value
         (or
          (pop asm-blox-test-random-values)
          0)))
    (if
        (and
         limit
         (> limit 0))
        (mod value limit)
      value)))

(defun asm-blox-test-sandbox-path
    (filename)
  (expand-file-name
   filename
   (getenv
    "NEOMACS_TEST_SANDBOX_ROOT")))

(defun asm-blox-test-to-index
    (row col)
  (+
   (* row asm-blox--gameboard-col-ct)
   col))

(defun asm-blox-test-runtime-summary
    (runtime)
  (list
   :row
   (asm-blox--cell-runtime-row runtime)
   :col
   (asm-blox--cell-runtime-col runtime)
   :pc
   (asm-blox--cell-runtime-pc runtime)
   :stack
   (asm-blox--cell-runtime-stack runtime)
   :ports
   (list
    (asm-blox--cell-runtime-up runtime)
    (asm-blox--cell-runtime-right runtime)
    (asm-blox--cell-runtime-down runtime)
    (asm-blox--cell-runtime-left runtime))
   :staging
   (list
    (asm-blox--cell-runtime-staging-up runtime)
    (asm-blox--cell-runtime-staging-right runtime)
    (asm-blox--cell-runtime-staging-down runtime)
    (asm-blox--cell-runtime-staging-left runtime))
   :state
   (asm-blox--cell-runtime-run-state runtime)))

(defun asm-blox-test-code-summary
    (node)
  (if
      (asm-blox-code-node-p node)
      (list
       (mapcar
        #'asm-blox-test-code-summary
        (asm-blox-code-node-children node))
       (asm-blox-code-node-start-pos node)
       (asm-blox-code-node-end-pos node))
    node))

(defun asm-blox-test-instruction-summary
    (instruction)
  (list
   (asm-blox-code-node-children instruction)
   (asm-blox-code-node-start-pos instruction)
   (asm-blox-code-node-end-pos instruction)))

(defun asm-blox-test-create-gameboard
    (cells)
  (let ((gameboard
         (make-vector
          (*
           asm-blox--gameboard-col-ct
           asm-blox--gameboard-row-ct)
          nil)))
    (dolist (cell cells)
      (let* ((row
              (nth 0 cell))
             (col
              (nth 1 cell))
             (text
              (nth 2 cell))
             (runtime
              (asm-blox--parse-cell
               (list row col)
               text)))
        (unless
            (asm-blox--cell-runtime-p runtime)
          (error
           "fixture parse failed: %S"
           runtime))
        (aset
         gameboard
         (asm-blox-test-to-index row col)
         runtime)))
    (dotimes (row asm-blox--gameboard-row-ct)
      (dotimes (col asm-blox--gameboard-col-ct)
        (let ((index
               (asm-blox-test-to-index row col)))
          (unless
              (aref gameboard index)
            (aset
             gameboard
             index
             (asm-blox--cell-runtime-create
              :instructions nil
              :pc 0
              :stack nil
              :row row
              :col col))))))
    gameboard))

(defun asm-blox-test-step
    (&optional count)
  (dotimes (_
            (or count 1))
    (asm-blox--gameboard-step)
    (asm-blox--resolve-port-values)))

(defun asm-blox-test-source-summary
    (source)
  (list
   (asm-blox--cell-source-row source)
   (asm-blox--cell-source-col source)
   (asm-blox--cell-source-data source)
   (asm-blox--cell-source-name source)
   (asm-blox--cell-source-idx source)))

(defun asm-blox-test-sink-summary
    (sink)
  (list
   (asm-blox--cell-sink-row sink)
   (asm-blox--cell-sink-col sink)
   (asm-blox--cell-sink-expected-data sink)
   (asm-blox--cell-sink-name sink)
   (asm-blox--cell-sink-idx sink)
   (asm-blox--cell-sink-err-val sink)
   (asm-blox--cell-sink-editor-text sink)
   (asm-blox--cell-sink-editor-point sink)
   (asm-blox--cell-sink-expected-text sink)))

(defun asm-blox-test-problem-summary
    (problem)
  (list
   (asm-blox--problem-spec-name problem)
   (asm-blox--problem-spec-difficulty problem)
   (mapcar
    #'asm-blox-test-source-summary
    (asm-blox--problem-spec-sources problem))
   (mapcar
    #'asm-blox-test-sink-summary
    (asm-blox--problem-spec-sinks problem))
   (asm-blox--problem-spec-description problem)
   (asm-blox--problem-spec-banned-commands problem)))

(defun asm-blox-test-problem-shape
    (problem)
  (list
   (asm-blox--problem-spec-name problem)
   (asm-blox--problem-spec-difficulty problem)
   (mapcar
    (lambda (source)
      (list
       (asm-blox--cell-source-row source)
       (asm-blox--cell-source-col source)
       (length
        (asm-blox--cell-source-data source))
       (asm-blox--cell-source-name source)))
    (asm-blox--problem-spec-sources problem))
   (mapcar
    (lambda (sink)
      (list
       (asm-blox--cell-sink-row sink)
       (asm-blox--cell-sink-col sink)
       (length
        (asm-blox--cell-sink-expected-data sink))
       (asm-blox--cell-sink-name sink)
       (and
        (asm-blox--cell-sink-editor-text sink)
        (length
         (asm-blox--cell-sink-editor-text sink)))
       (and
        (asm-blox--cell-sink-expected-text sink)
        (length
         (asm-blox--cell-sink-expected-text sink)))))
    (asm-blox--problem-spec-sinks problem))
   (asm-blox--problem-spec-banned-commands problem)))

(defun asm-blox-test-fixture-problem
    ()
  (asm-blox--problem-spec-create
   :name "Fixture Board"
   :difficulty 'medium
   :sources
   (list
    (asm-blox--cell-source-create
     :row -1 :col 0
     :data '(4 5)
     :idx 0
     :name "I"))
   :sinks
   (list
    (asm-blox--cell-sink-create
     :row 3 :col 3
     :expected-data '(9)
     :name "O"))
   :description
   "Exercise a practical editable board."
   :banned-commands '(DIV)))

(defun asm-blox-test-prepare-edit-buffer
    (&optional entries)
  (setq
   asm-blox--extra-gameboard-cells
   (asm-blox-test-fixture-problem)
   asm-blox--display-mode 'edit
   asm-blox--disable-redraw t
   asm-blox-box-contents
   (make-hash-table :test 'equal)
   asm-blox--beginning-of-box-points
   (make-hash-table :test 'equal)
   asm-blox--end-of-box-points
   (make-hash-table :test 'equal))
  (dotimes (row asm-blox--gameboard-row-ct)
    (dotimes (col asm-blox--gameboard-col-ct)
      (puthash
       (list row col)
       ""
       asm-blox-box-contents)))
  (dolist (entry entries)
    (puthash
     (list
      (nth 0 entry)
      (nth 1 entry))
     (nth 2 entry)
     asm-blox-box-contents))
  (asm-blox-display-game-board))
"##;

fn asm_blox_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASM_BLOX_MELPA_PIN, source_file)
        .expect("prepare pinned asm-blox source below ./tmp")
        .with_prelude(ASM_BLOX_TEST_PRELUDE)
        .with_timeout(ASM_BLOX_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed asm-blox parity test")
        .into()
}

/// Multi-probe batch for `assert_asm_blox_autoload_parity` cases (2a).
pub(crate) fn assert_asm_blox_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asm_blox_oracle("asm-blox-autoloads.el"),
        &name,
        "asm_blox_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_asm_blox_parity` cases (2a).
pub(crate) fn assert_asm_blox_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        asm_blox_oracle("asm-blox.el"),
        &name,
        "asm_blox_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn asm_blox_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_asm_blox_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_asm_blox_autoload_batch(&cases);
}

#[test]
fn asm_blox_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        files::files_public_surface_batch_cases(),
        parser::parser_public_surface_batch_cases(),
        puzzles::puzzles_public_surface_batch_cases(),
        registry::registry_asm_blox_batch_cases(),
        runtime::runtime_public_surface_batch_cases(),
        sources_sinks::sources_sinks_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
        yaml_cells::yaml_cells_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_asm_blox_batch(&cases);
}

// END generated package batch tests
