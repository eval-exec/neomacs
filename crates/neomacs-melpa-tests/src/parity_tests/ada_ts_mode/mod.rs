use std::time::Duration;

use crate::{
    ADA_TS_MODE_MELPA_PIN, CachedMelpaOracle, EmacsRuntime, elisp_string,
    prepare_cached_tree_sitter_grammar,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADA_TS_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ADA_TREE_SITTER_REPOSITORY: &str = "https://github.com/briot/tree-sitter-ada";
const ADA_TREE_SITTER_REVISION: &str = "6b58259a08b1a22ba0247a7ce30be384db618da6";

/// ada-ts-mode is a tree-sitter major mode, so every workflow needs the real
/// pinned Ada grammar: it is fetched at the exact revision above, built below
/// `./tmp`, and put on `treesit-extra-load-path`.  The mode itself refuses to
/// start unless `treesit-ready-p` is true, so a passing workflow is proof that
/// the grammar loaded and a real parse tree exists -- there is no non-treesit
/// fallback path that could quietly satisfy these tests.  Real `.ads`/`.adb`
/// files are written into the per-case sandbox and visited normally.
const ADA_TS_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'treesit)

(setq make-backup-files nil create-lockfiles nil)

(defvar ada-test-root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ada-test-write (name text)
  (let ((path (expand-file-name name ada-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defconst ada-test-spec
  "--  Inventory management for the demo shop.
package Shop.Inventory is

   Max_Items : constant Natural := 100;

   type Item_Id is new Positive;

   function Name_Of (Id : Item_Id) return String;

   procedure Restock (Id : Item_Id; Count : Natural);

end Shop.Inventory;
")

(defconst ada-test-body
  "package body Shop.Inventory is

   function Name_Of (Id : Item_Id) return String is
   begin
      return \"Artikel\";
   end Name_Of;

   procedure Restock (Id : Item_Id; Count : Natural) is
      Remaining : Natural := Count;
   begin
      while Remaining > 0 loop
         Remaining := Remaining - 1;
      end loop;
   end Restock;

end Shop.Inventory;
")

(defmacro ada-test-in-file (name text &rest body)
  `(let* ((path (ada-test-write ,name ,text))
          (buffer (find-file-noselect path)))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer)))))

(defun ada-test-faces-at (needles)
  "Face at the start of each NEEDLE, searched in order from point-min."
  (mapcar (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (goto-char (match-beginning 0))
            (list needle (point) (get-text-property (point) 'face)))
          needles))

(defun ada-test-flatten-index (node)
  "NODE with every marker replaced by its position and strings unpropertized."
  (cond
   ((markerp node) (marker-position node))
   ((stringp node) (substring-no-properties node))
   ((consp node) (cons (ada-test-flatten-index (car node))
                       (ada-test-flatten-index (cdr node))))
   (t node)))
"##;

fn ada_ts_mode_oracle() -> CachedMelpaOracle {
    let grammar_dir = prepare_cached_tree_sitter_grammar(
        &EmacsRuntime::gnu_emacs(),
        "ada",
        ADA_TREE_SITTER_REPOSITORY,
        ADA_TREE_SITTER_REVISION,
    )
    .expect("prepare pinned Ada Tree-sitter grammar below ./tmp");
    let grammar_dir = elisp_string(&grammar_dir.to_string_lossy());
    CachedMelpaOracle::new(ADA_TS_MODE_MELPA_PIN, "ada-ts-mode.el")
        .expect("prepare pinned ada-ts-mode source below ./tmp")
        .with_prelude(format!(
            "(setq treesit-extra-load-path (list {grammar_dir}))\n{ADA_TS_MODE_TEST_PRELUDE}"
        ))
        .with_timeout(ADA_TS_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ada-ts-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_ada_ts_mode_parity` cases (2a).
pub(crate) fn assert_ada_ts_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ada_ts_mode_oracle(), &name, "ada_ts_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ada_ts_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ada_ts_mode_batch(&cases);
}

// END generated package batch tests
