use super::*;

/// `primitive-undo` is Lisp, and only Lisp.
///
/// GNU's `syms_of_undo` (src/undo.c:423-490) has exactly one `defsubr`,
/// `&Sundo_boundary` (:435, its DEFUN at src/undo.c:251); there is no
/// `Fprimitive_undo` anywhere in src/.  The function is
/// `(defun primitive-undo (n list) ...)` at lisp/simple.el:3645, so before
/// that file is loaded nothing answers the name at all, and after it is
/// loaded the function cell holds a byte-code function.
///
/// Neomacs used to register a Rust subr under this name.  `defun` shadowed it
/// in every real session, which is why the wrongness below never showed --
/// but a bare `Context` is not a real session, and the Rust arms it exposed
/// there disagreed with GNU in both directions (DIVERGENCES.md 146).
#[test]
fn primitive_undo_is_lisp_only_like_gnu() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    // Before any Lisp is loaded: `undo-boundary' answers (GNU has it in C),
    // `primitive-undo' does not (GNU has no C version).
    let mut bare = Context::new();
    assert_eq!(
        super::super::print::print_value(&bare.eval_str("(fboundp 'undo-boundary)").unwrap()),
        "t"
    );
    assert_eq!(
        super::super::print::print_value(&bare.eval_str("(fboundp 'primitive-undo)").unwrap()),
        "nil"
    );

    // After loadup: simple.el's definition, and it is not a subr.
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            "(fboundp 'primitive-undo)
             (subrp (symbol-function 'primitive-undo))
             (func-arity (symbol-function 'primitive-undo))",
        ),
        vec!["OK t", "OK nil", "OK (2 . 2)"],
    );
}

/// Everything the deleted Rust subr used to answer, measured under GNU Emacs
/// 31.0.90 `-Q --batch` and re-asked of the runtime, where lisp/simple.el's
/// `primitive-undo' is what replies.
///
/// The rows marked DIVERGED are the ones the deleted Rust subr got wrong.
#[test]
fn primitive_undo_entry_arms_match_gnu() {
    crate::test_utils::init_test_tracing();

    let results = crate::test_utils::runtime_startup_eval_all(
        r#"
;; A group is terminated by ONE boundary, not by a run of them.  DIVERGED:
;; the Rust subr skipped every leading nil before counting a group.
(primitive-undo 1 (list nil nil nil))
(primitive-undo 2 (list nil nil nil))
;; COUNT <= 0 consumes nothing.
(primitive-undo 0 (list nil nil))
(primitive-undo -5 (list nil))
;; COUNT is compared with `>', so a float is accepted.  DIVERGED: the Rust
;; subr signalled `(wrong-type-argument integerp 1.5)'.
(primitive-undo 1.5 (list nil nil nil))
;; A non-list LIST fails in `pop', i.e. as `listp', not before it.
(condition-case e (primitive-undo 1 7) (error e))
;; Arity.
(condition-case e (funcall 'primitive-undo 1) (error e))
(condition-case e (funcall 'primitive-undo) (error e))
(condition-case e (funcall 'primitive-undo 1 nil nil) (error e))
;; (BEG . END): undo an insertion.
(with-temp-buffer (insert "hello") (primitive-undo 1 (list (cons 1 6))) (buffer-string))
;; (TEXT . POS): undo a deletion; POS's sign chooses where point lands.
(with-temp-buffer (primitive-undo 1 (list (cons "hello" 1))) (list (buffer-string) (point)))
(with-temp-buffer (primitive-undo 1 (list (cons "hello" -1))) (list (buffer-string) (point)))
;; An integer entry is `goto-char'.
(with-temp-buffer (insert "hello") (primitive-undo 1 (list 3)) (point))
;; (TEXT . POS) followed by (MARKER . OFFSET).
(with-temp-buffer
  (insert "ae")
  (let ((m (copy-marker 2 nil)))
    (primitive-undo 1 (list (cons "bcd" 2) (cons m -2)))
    (list (buffer-string) (marker-position m))))
;; A raw byte goes back into a unibyte buffer as that byte.
(with-temp-buffer
  (set-buffer-multibyte nil)
  (primitive-undo 1 (list (cons (unibyte-string 255) 1)))
  (list (multibyte-string-p (buffer-string)) (append (buffer-string) nil)))
;; (nil PROP VAL BEG . END) restores a property, including a nil VAL.
(with-temp-buffer
  (insert "abcd")
  (put-text-property 2 4 'face 'bold)
  (primitive-undo 1 (list '(nil face nil 2 . 4)))
  (list (get-text-property 1 'face) (get-text-property 2 'face) (get-text-property 3 'face)))
;; Re-inserted TEXT keeps the properties the string carries.
(with-temp-buffer
  (primitive-undo 1 (list (cons (propertize "abc" 'face 'bold) 1)))
  (list (buffer-string) (get-text-property 1 'face) (get-text-property 3 'face)))
;; A range outside the accessible portion is an error, not a clamp.
(with-temp-buffer
  (insert "hello")
  (narrow-to-region 2 4)
  (condition-case e (primitive-undo 1 (list (cons 1 6))) (error e)))
;; (apply FUN . ARGS) is called.
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (set-buffer-multibyte nil)
  (let ((ul buffer-undo-list))
    (list ul (progn (primitive-undo 1 ul) enable-multibyte-characters))))
;; An entry of no known shape is an error.  DIVERGED: the Rust subr skipped it.
(with-temp-buffer (condition-case e (primitive-undo 1 (list (vector 1 2))) (error e)))
"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch (tmp/pw52-gnu*.el).
    assert_eq!(
        results,
        vec![
            "OK (nil nil)",
            "OK (nil)",
            "OK (nil nil)",
            "OK (nil)",
            "OK (nil)",
            "OK (wrong-type-argument listp 7)",
            "OK (wrong-number-of-arguments (2 . 2) 1)",
            "OK (wrong-number-of-arguments (2 . 2) 0)",
            "OK (wrong-number-of-arguments (2 . 2) 3)",
            "OK \"\"",
            "OK (\"hello\" 1)",
            "OK (\"hello\" 6)",
            "OK 3",
            "OK (\"abcde\" 4)",
            "OK (nil (255))",
            "OK (nil nil nil)",
            "OK (#(\"abc\" 0 3 (face bold)) bold bold)",
            "OK (error \"Changes to be undone are outside visible portion of buffer\")",
            "OK (((apply set-buffer-multibyte t)) t)",
            "OK (error \"Unrecognized entry in undo list [1 2]\")",
        ],
    );
}

/// The `(t . MODTIME)` arm, which is the one ledger 145 handed forward.
///
/// GNU compares the recorded time against `(visited-file-modtime)` with
/// `time-equal-p' and only then clears the modified flag
/// (lisp/simple.el:3668-3688).  The Rust subr instead cleared the flag
/// whenever the recorded value was the fixnum 0 and skipped every other
/// value, so it was wrong in BOTH directions -- see DIVERGENCES.md 146 for
/// the measured before/after rows.
#[test]
fn primitive_undo_modtime_arm_compares_against_the_visited_file_like_gnu() {
    crate::test_utils::init_test_tracing();

    let dir = std::env::temp_dir().join(format!("neo-pu-modtime-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let dir = dir.to_string_lossy().to_string();

    let results = crate::test_utils::runtime_startup_eval_all(&format!(
        r#"
;; A recorded 0 against a buffer that HAS a modtime is an obsolete save:
;; the flag must stay set.
(let ((f "{dir}/a.txt"))
  (with-temp-file f (insert "hello\n"))
  (with-current-buffer (find-file-noselect f)
    (setq buffer-undo-list t)
    (insert "X")
    (set-buffer-modified-p t)
    (primitive-undo 1 (list (cons t 0)))
    (prog1 (buffer-modified-p) (set-buffer-modified-p nil) (kill-buffer))))
;; A visited file that does not exist records -1, and -1 matches it.
(let ((f "{dir}/b.txt"))
  (when (file-exists-p f) (delete-file f))
  (with-current-buffer (find-file-noselect f)
    (setq buffer-undo-list t)
    (insert "X")
    (set-buffer-modified-p t)
    (prog1 (list (visited-file-modtime)
                 (progn (primitive-undo 1 (list (cons t -1))) (buffer-modified-p)))
      (set-buffer-modified-p nil) (kill-buffer))))
;; A recorded timestamp equal to the buffer's own clears the flag.
(let ((f "{dir}/c.txt"))
  (with-temp-file f (insert "hello\n"))
  (with-current-buffer (find-file-noselect f)
    (setq buffer-undo-list t)
    (let ((mt (visited-file-modtime)))
      (insert "X")
      (set-buffer-modified-p t)
      (primitive-undo 1 (list (cons t mt)))
      (prog1 (buffer-modified-p) (set-buffer-modified-p nil) (kill-buffer)))))
"#
    ));

    // Transcribed from GNU Emacs 31.0.90 -Q --batch (tmp/pw52-gnu-probe.el).
    assert_eq!(results, vec!["OK t", "OK (-1 nil)", "OK nil"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_undo_boundary_no_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_undo_boundary(&mut eval, vec![]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_undo_boundary_eval_inserts_boundary_marker() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert("x");
    }
    let result = builtin_undo_boundary(&mut eval, vec![]);
    assert!(result.is_ok());
    let buffer = eval.buffers.current_buffer().expect("scratch buffer");
    let ul = buffer.get_undo_list();
    assert!(crate::buffer::undo_list_has_trailing_boundary(&ul));
}

#[test]
fn test_undo_boundary_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = builtin_undo_boundary(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn test_delete_records_marker_adjustments_for_primitive_undo() {
    crate::test_utils::init_test_tracing();

    // `primitive-undo' is lisp/simple.el's, so this needs the loaded runtime.
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "ABCDE")
  (let ((m (make-marker)))
    (set-marker m 3)
    (undo-boundary)
    (goto-char 3)
    (insert "123")
    (undo-boundary)
    (delete-region 1 3)
    (let ((after-delete (marker-position m)))
      (primitive-undo 1 buffer-undo-list)
      (list after-delete (marker-position m) (buffer-string)))))"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch (tmp/pw52-gnu4.el).
    assert_eq!(result, r#"OK (1 3 "AB123CDE")"#);
}

#[test]
fn replace_match_undo_keeps_overlay_endpoint_like_gnu() {
    crate::test_utils::init_test_tracing();

    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "hello WORLD hello WORLD hello")
  (let ((ov1 (make-overlay 1 12))
        (ov2 (make-overlay 13 25)))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "WORLD" nil t)
      (replace-match "UNIVERSE" t t))
    (let ((after-replace (list (overlay-start ov1) (overlay-end ov1)
                               (overlay-start ov2) (overlay-end ov2))))
      (primitive-undo 1 buffer-undo-list)
      (list after-replace
            (buffer-string)
            (overlay-start ov1) (overlay-end ov1)
            (overlay-start ov2) (overlay-end ov2)))))"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch (tmp/pw52-gnu4.el).
    assert_eq!(
        result,
        r#"OK ((1 15 16 31) "hello WORLD hello WORLD hello" 1 12 13 25)"#
    );
}

#[test]
fn transpose_regions_undo_records_equal_regions_like_gnu() {
    crate::test_utils::init_test_tracing();

    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "AAA-BBB-CCC")
  (let ((m (copy-marker 5 t)))
    (undo-boundary)
    (transpose-regions 1 3 5 7)
    (let ((after (list (buffer-string) (marker-position m))))
      (primitive-undo 1 buffer-undo-list)
      (list after (buffer-string) (marker-position m)))))"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch (tmp/pw52-gnu4.el).
    assert_eq!(result, r#"OK (("BBA-AAB-CCC" 1) "AAA-BBB-CCC" 3)"#);
}

/// The nine bare-`Context` tests that used to sit here called
/// `builtin_undo` -- a Rust `undo` subr of a function GNU implements only in
/// `lisp/simple.el:3466`, whose replay was `BufferManager::undo_buffer`, a
/// third undo loop with no evaluator.  Subr and loop are both deleted
/// (DIVERGENCES.md 150), so what is left to state is what the RECORDER
/// produces and what `lisp/simple.el`'s `undo` makes of it.
///
/// Four of the nine pinned arity and error shapes the Lisp `defun` answers
/// differently; those moved to `undo_arms_match_gnu`
/// (`builtins/lisp_only_undo_commands_test.rs`).  The five that were really
/// about the recorded entries are below, re-measured under GNU Emacs 31.0.90
/// `-Q --batch` first (tmp/pw56-moved-tests-gnu.txt).
#[test]
fn undo_replays_the_recorded_entries_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = crate::test_utils::runtime_startup_eval_all(
        r#"
;; A plain insertion.
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "abc")
  (undo-boundary)
  (setq last-command nil)
  (undo)
  (buffer-string))
;; `remove-text-properties' over a range whose START was unpropertied: the
;; recorded entry must restore the property only where it was.
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 3 5 'face 'bold)
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (remove-text-properties 1 5 '(face nil))
  (undo-boundary)
  (setq last-command nil)
  (undo)
  (list (get-text-property 1 'face) (get-text-property 2 'face)
        (get-text-property 3 'face) (get-text-property 4 'face)
        (get-text-property 5 'face)))
;; `set-text-properties' across a partial interval.
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 3 5 'face 'italic)
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (set-text-properties 2 4 '(face underline))
  (undo-boundary)
  (setq last-command nil)
  (undo)
  (list (get-text-property 1 'face) (get-text-property 2 'face)
        (get-text-property 3 'face) (get-text-property 4 'face)
        (get-text-property 5 'face)))
;; ARG 0: `prefix-numeric-value' passes it through and `primitive-undo'
;; compares COUNT with `>', so nothing is undone and `undo' still succeeds.
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (insert "one")
  (undo-boundary)
  (setq last-command nil)
  (list (undo 0) (buffer-string)))
"#,
    );

    // Transcribed from GNU Emacs 31.0.90 -Q --batch.
    assert_eq!(
        results,
        vec![
            "OK \"\"",
            "OK (nil nil bold bold nil)",
            "OK (bold nil nil italic nil)",
            "OK (\"Undo\" \"one\")",
        ],
    );
}

/// Return the printed form of the current buffer's `buffer-undo-list` in a bare
/// `Context` (whose default `buffer-undo-list` is nil = enabled).
fn undo_list_after(eval: &mut super::super::eval::Context) -> String {
    let value = eval.eval_str("buffer-undo-list").expect("buffer-undo-list");
    super::super::print::print_value(&value)
}

/// GNU `record_first_change` (undo.c:64,210) pushes `(t . MODTIME)` whenever
/// the buffer transitions clean->modified, i.e. on every edit while
/// `MODIFF <= SAVE_MODIFF`.  `(set-buffer-modified-p nil)` resets
/// `SAVE_MODIFF = MODIFF`, so the buffer becomes clean again and the *next*
/// modification must re-emit the first-change marker.  neomacs previously
/// emitted it only once (a sticky `recorded_first_change` flag) and never
/// re-armed, producing `((8 . 9) (3 . 4) (1 . 11) (t . 0))` instead of GNU's
/// `((8 . 9) (3 . 4) (t . 0) (1 . 11) (t . 0))`.
///
/// Driven through the bare `Context` buffer (whose default `buffer-undo-list`
/// is nil = enabled) because the heavier bootstrap harness is unavailable in
/// this build; the recorder path exercised is identical.
#[test]
fn first_change_marker_rearmed_after_set_modified_nil() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();
    eval.eval_str("(insert \"0123456789\")").unwrap();
    eval.eval_str("(set-buffer-modified-p nil)").unwrap();
    eval.eval_str("(goto-char 3)").unwrap();
    eval.eval_str("(insert \"A\")").unwrap();
    eval.eval_str("(goto-char 8)").unwrap();
    eval.eval_str("(insert \"B\")").unwrap();
    assert_eq!(
        undo_list_after(&mut eval),
        "((8 . 9) (3 . 4) (t . 0) (1 . 11) (t . 0))"
    );
}

/// Without re-cleaning between edits, GNU records the first-change marker only
/// once: after the first edit increments `MODIFF`, later edits in the same
/// modified run see `MODIFF > SAVE_MODIFF` and skip the marker.  Guards against
/// over-eager re-arming (a regression in the other direction).  The original
/// `(1 . 11)` insert and its first-change marker remain on the list because
/// `set-buffer-modified-p nil` does not clear `buffer-undo-list`.
#[test]
fn first_change_marker_recorded_once_per_modified_run() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();
    eval.eval_str("(insert \"0123456789\")").unwrap();
    eval.eval_str("(set-buffer-modified-p nil)").unwrap();
    eval.eval_str("(insert \"X\")").unwrap();
    eval.eval_str("(insert \"Y\")").unwrap();
    // "X"/"Y" coalesce into one (11 . 13) entry, preceded by a single re-armed
    // first-change marker for the clean->modified transition.
    assert_eq!(
        undo_list_after(&mut eval),
        "((11 . 13) (t . 0) (1 . 11) (t . 0))"
    );
}

/// Re-cleaning between every edit re-arms the first-change marker each time,
/// matching GNU's per-transition behavior.
#[test]
fn first_change_marker_rearmed_on_each_clean_transition() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();
    eval.eval_str("(insert \"0123456789\")").unwrap();
    eval.eval_str("(set-buffer-modified-p nil)").unwrap();
    eval.eval_str("(insert \"X\")").unwrap();
    eval.eval_str("(set-buffer-modified-p nil)").unwrap();
    eval.eval_str("(insert \"Y\")").unwrap();
    assert_eq!(
        undo_list_after(&mut eval),
        "((12 . 13) (t . 0) (11 . 12) (t . 0) (1 . 11) (t . 0))"
    );
}

/// Back-to-front edits, the shape every LSP-style client applies (`tide` walks
/// TypeScript's `textChanges` in reverse so earlier positions stay valid), must
/// undo to the original text.
///
/// GNU records the two adjacent one-character insertions as `(19 . 20)` and
/// `(20 . 21)` -- `record_insert` (src/undo.c:98-112) coalesces only when the
/// newest record ENDS where the new insertion BEGINS.  neomacs also coalesced
/// in the opposite direction, producing a single `(19 . 21)` whose undo
/// deleted the untouched `=` between the two inserted spaces and turned
/// `total=add` into `total add`.
#[test]
fn undo_of_descending_adjacent_inserts_restores_the_untouched_text() {
    crate::test_utils::init_test_tracing();

    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
                 (buffer-enable-undo)
                 (setq buffer-undo-list nil)
                 (insert "export const total=add(1,2)")
                 (undo-boundary)
                 (goto-char 20)
                 (insert " ")
                 (goto-char 19)
                 (insert " ")
                 (let ((records buffer-undo-list))
                   (primitive-undo 1 buffer-undo-list)
                   (list (buffer-string) (car records) (car (cdr records)))))"#,
    );

    // Transcribed from GNU Emacs -Q --batch (tmp/p97-probe8.el, re-measured
    // in tmp/pw52-gnu4.el).
    assert_eq!(
        result,
        "OK (\"export const total=add(1,2)\" (19 . 20) (20 . 21))"
    );
}

/// GNU `recursive_edit_1` (keyboard.c:708-748) specbinds
/// `undo-auto--undoably-changed-buffers` to nil before it runs the command
/// loop, "so that changes in the recursive edit will not result in undo
/// boundaries in buffers changed before we entered there recursive edit"
/// (keyboard.c:741-747, Bug #23632).  Both `recursive-edit` and `read_minibuf`
/// enter through it, so reading an argument from the minibuffer must not drop a
/// boundary into a buffer an earlier, already-finished command edited.
///
/// Measured on GNU Emacs -Q --batch (tmp/p97-probe11.el): the undo list is
/// `((1 . 6) (t . 0))` both before and after the read.
#[test]
fn a_minibuffer_read_adds_no_undo_boundary_to_previously_changed_buffers() {
    crate::test_utils::init_test_tracing();

    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval
        .eval_str(
            r#"(let ((buf (get-buffer-create "neo-undo-boundary")))
                 (set-buffer buf)
                 (buffer-enable-undo)
                 (setq buffer-undo-list nil)
                 (insert "hello")
                 (let ((setup
                        (lambda ()
                          (setq unread-command-events
                                (append (listify-key-sequence (kbd "z RET"))
                                        unread-command-events)))))
                   (add-hook 'minibuffer-setup-hook setup)
                   (unwind-protect
                       (let ((executing-kbd-macro t))
                         (read-from-minibuffer "P: "))
                     (remove-hook 'minibuffer-setup-hook setup)))
                 (with-current-buffer buf
                   (let ((boundaries 0))
                     (dolist (entry buffer-undo-list)
                       (unless entry (setq boundaries (1+ boundaries))))
                     (list boundaries (length buffer-undo-list)))))"#,
        )
        .expect("minibuffer read should finish");

    assert_eq!(format!("{result}"), "(0 2)");
}

/// GNU's first-change entry is `(t . VISITED-FILE-MODTIME)`, not a constant:
/// `record_first_change` stores `buffer_visited_file_modtime (base_buffer)`
/// (`src/undo.c:209-223`), and `primitive-undo`'s `(t . TIME)` arm
/// (`lisp/simple.el:3669-3688`) clears the modified flag only when
/// `(time-equal-p time (visited-file-modtime))` -- so that undoing back to a
/// save the file has since outlived does NOT claim the buffer is unmodified.
///
/// Neomacs recorded the fixnum 0 for every buffer, so a file-visiting buffer's
/// comparison could never match and `undo` back to the saved text left
/// `buffer-modified-p` t where GNU reports nil.  The org-ref completion
/// workflow reads exactly that flag after `C-_`.
///
/// This pins the recorder, which is the part Neomacs owns -- `primitive-undo`
/// itself is GNU's `simple.el` and is not loaded in a bare `Context`.  The
/// entry's datum must be exactly what `visited-file-modtime` reports for the
/// same buffer, and for a file-visiting buffer that is a timestamp, never the
/// no-modtime 0.  The full flow reads
/// `(:before-undo t :text "" :after-undo nil :modtime-recorded t)` under GNU
/// Emacs 31.0.90.
#[test]
fn first_change_marker_records_the_visited_file_modtime_like_gnu() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-undo-first-change-{unique}"));
    std::fs::create_dir_all(&dir).expect("create temp fixture dir");
    let file = dir.join("probe.txt");
    std::fs::write(&file, "").expect("write empty fixture");

    let form = format!(
        r#"(progn
             (setq buffer-file-name {:?})
             (set-visited-file-modtime)
             (setq buffer-undo-list nil)
             (set-buffer-modified-p nil)
             (insert "hello")
             (list :recorded (cdr (assq t buffer-undo-list))
                   :matches (equal (cdr (assq t buffer-undo-list))
                                   (visited-file-modtime))))"#,
        file.to_string_lossy()
    );
    let rendered = super::super::print::print_value(&eval.eval_str(&form).expect("eval"));
    assert!(
        rendered.ends_with(":matches t)"),
        "the first-change datum must be the buffer's visited-file modtime: {rendered}"
    );
    assert!(
        !rendered.starts_with("(:recorded 0 "),
        "a file-visiting buffer records a timestamp, not GNU's no-modtime 0: {rendered}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Ledger 105's residual.  GNU's `record_first_change` resolves the BASE
/// buffer before reading a modtime (`src/undo.c:209-223`):
///
/// ```c
///   struct buffer *base_buffer = current_buffer;
///   ...
///   if (base_buffer->base_buffer)
///     base_buffer = base_buffer->base_buffer;
///   bset_undo_list (current_buffer,
///                   Fcons (Fcons (Qt, buffer_visited_file_modtime (base_buffer)),
///                          BVAR (current_buffer, undo_list)));
/// ```
///
/// The redirect is unconditional, not a fallback for a missing value: under
/// GNU 31.0.90 an indirect buffer that was given a modtime of its own with
/// `(set-visited-file-modtime '(1 2 3 4))` STILL records its base's.  This
/// fixture pins that, because "record the buffer's own modtime, whatever it
/// is" passes the weaker `(t . 0)` version of the test.
///
/// The base's modtime is also read live, at the moment of the change -- so it
/// is set here AFTER the indirect buffer exists, the order `find-file` +
/// `clone-indirect-buffer` + `save-buffer` produces.
#[test]
fn first_change_in_an_indirect_buffer_records_its_base_buffers_modtime() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();

    let form = r#"(let ((base (get-buffer-create "base-145")))
             (set-buffer base)
             (setq buffer-file-name "/nonesuch/base-145.txt")
             (set-buffer (make-indirect-buffer base "indirect-145"))
             (set-visited-file-modtime '(1 2 3 4))
             (set-buffer base)
             (set-visited-file-modtime '(25000 1000 0 0))
             (set-buffer-modified-p nil)
             (setq buffer-undo-list nil)
             (set-buffer "indirect-145")
             (insert "X")
             (list :recorded (cdr (assq t buffer-undo-list))
                   :own-modtime (visited-file-modtime)))"#;
    let rendered = super::super::print::print_value(&eval.eval_str(form).expect("eval"));
    assert_eq!(
        rendered, "(:recorded (25000 1000 0 0) :own-modtime (1 2 3 0))",
        "the first-change entry must hold the BASE buffer's modtime, not the \
         indirect buffer's own: {rendered}"
    );
}

/// GNU records `-1`, not `0`, for a buffer visiting a file that does not
/// exist: `insert-file-contents` stores `time_error_value (save_errno)`
/// (`src/fileio.c:3971-3978,4200`) before it signals, and
/// `buffer_visited_file_modtime` maps that `NONEXISTENT_MODTIME_NSECS` to the
/// fixnum -1 (`src/fileio.c:6156-6163`).  Every `find-file` of a new file
/// lands here, so the first change to one records `(t . -1)` under GNU
/// 31.0.90 where Neomacs recorded `(t . 0)`.
#[test]
fn a_visited_file_that_does_not_exist_records_minus_one() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();

    let form = r#"(progn
             (condition-case nil
                 (insert-file-contents "/nonesuch/no-such-file-145.txt" t)
               (error nil))
             (setq buffer-undo-list nil)
             (set-buffer-modified-p nil)
             (insert "X")
             (list :own-modtime (visited-file-modtime)
                   :recorded (cdr (assq t buffer-undo-list))
                   :verify (verify-visited-file-modtime (current-buffer))))"#;
    let rendered = super::super::print::print_value(&eval.eval_str(form).expect("eval"));
    assert_eq!(
        rendered, "(:own-modtime -1 :recorded -1 :verify t)",
        "a nonexistent visited file is GNU's -1 modtime, and it still verifies \
         while the file is still missing: {rendered}"
    );
}

/// GNU `record_point` (`src/undo.c:47-78`) reads `at_boundary` from the undo
/// list BEFORE `record_first_change` may cons `(t . TIME)` onto it, and only
/// then pushes `point_before_last_command_or_undo`.  GNU's own comment on that
/// read names the hazard: "This check is currently dependent on being called
/// before record_first_change".
///
/// Neomacs ran the first-change step first and let each recorder re-derive
/// `at_boundary` afterwards, so on the very case the check exists for -- the
/// first change to a CLEAN buffer -- the list was no longer at a boundary and
/// the point entry was dropped.  `primitive-undo` then had nothing to restore
/// point from and `undo` left point at the change instead of where the command
/// started (Tide's format/undo-only workflow read point 206 for GNU's 1).
///
/// The expected list is what GNU Emacs 31.0.90 prints for this exact form
/// under `emacs -Q --batch`:
///
/// ```text
/// undo-list=((7 . 8) 1 (t 27263 42178 675008 795000))
/// ```
#[test]
fn record_point_runs_before_the_first_change_sentinel_like_gnu() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;
    let mut eval = Context::new();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-undo-record-point-{unique}"));
    std::fs::create_dir_all(&dir).expect("create temp fixture dir");
    let file = dir.join("probe.txt");
    std::fs::write(&file, "").expect("write empty fixture");

    let form = format!(
        r#"(progn
             (setq buffer-file-name {:?})
             (set-visited-file-modtime)
             (insert "abcdef")
             (setq buffer-undo-list nil)
             (set-buffer-modified-p nil)
             (goto-char (point-min))
             (undo-boundary)
             (save-excursion (goto-char (point-max)) (insert "X"))
             (list :newest (car buffer-undo-list)
                   :point-entry (nth 1 buffer-undo-list)
                   :first-change (car-safe (nth 2 buffer-undo-list))
                   :length (length buffer-undo-list)))"#,
        file.to_string_lossy()
    );
    let rendered = super::super::print::print_value(&eval.eval_str(&form).expect("eval"));
    assert_eq!(
        rendered, "(:newest (7 . 8) :point-entry 1 :first-change t :length 3)",
        "the point entry belongs between the change and the first-change sentinel"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Full-runtime helper: the three pins below need the real command loop,
/// because what they are about is a variable the command loop binds.
fn undo_bootstrap_eval_one(src: &str) -> String {
    crate::test_utils::runtime_startup_eval_all(src)
        .into_iter()
        .next()
        .expect("at least one form")
}

/// Open a buffer that `execute-kbd-macro' can actually type into.
///
/// `execute-kbd-macro' reaches the buffer of the *selected window*, so making
/// the buffer merely current is not enough.
const UNDO_TYPING_PRELUDE: &str = r#"
(defun neo-undo-pin-open (col)
  (let ((buffer (generate-new-buffer "*undo-pin*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (text-mode)
    (buffer-enable-undo)
    (setq fill-column col)
    buffer))
"#;

/// GNU `recursive_edit_1' (src/keyboard.c:747) specbinds
/// `undo-auto--undoably-changed-buffers' to nil ONCE, at the recursive-edit
/// entry both `recursive-edit' and `read_minibuf' pass through -- never in
/// `command_loop_1'.  The distinction is load bearing: `execute-kbd-macro'
/// enters `command_loop_1' but NOT `recursive_edit_1', so the buffers a macro's
/// last command changed must stay on the list after the macro returns.
///
/// `undo-auto--boundaries' adds a boundary to every buffer on that list
/// (lisp/simple.el:4106-4116).  Lose the list at the macro's edge and the next
/// command adds no boundary at all, because an amalgamating cause never
/// re-adds the current buffer.
///
/// Measured on GNU Emacs -Q --batch: `(nil (1 . 4) (t . 0))'.
#[test]
fn a_command_after_a_keyboard_macro_gets_the_undo_boundary_for_the_macro() {
    crate::test_utils::init_test_tracing();
    let result = undo_bootstrap_eval_one(&format!(
        r#"(progn
             {UNDO_TYPING_PRELUDE}
             (let ((buffer (neo-undo-pin-open 20)))
               (execute-kbd-macro (string-to-vector "abc"))
               (let ((head-after-typing (car buffer-undo-list)))
                 (execute-kbd-macro (kbd "C-a"))
                 (list head-after-typing
                       (car buffer-undo-list)
                       buffer-undo-list))))"#
    ));
    assert_eq!(result, "OK ((1 . 4) nil (nil (1 . 4) (t . 0)))");
}

/// The same boundary keeps two typing runs from collapsing into one record.
///
/// `record_insert' (src/undo.c:98-112) coalesces into the newest record when it
/// is an insertion ending where the new one begins.  The boundary the second
/// macro's first command adds is what stops that: without it the two runs merge
/// into a single `(1 . 7)' and one undo takes back both.
///
/// Measured on GNU Emacs -Q --batch: `((4 . 7) nil (1 . 4) (t . 0))'.
#[test]
fn two_typing_runs_stay_two_undo_records_separated_by_a_boundary() {
    crate::test_utils::init_test_tracing();
    let result = undo_bootstrap_eval_one(&format!(
        r#"(progn
             {UNDO_TYPING_PRELUDE}
             (let ((buffer (neo-undo-pin-open 20)))
               (execute-kbd-macro (string-to-vector "abc"))
               (execute-kbd-macro (string-to-vector "def"))
               buffer-undo-list))"#
    ));
    assert_eq!(result, "OK ((4 . 7) nil (1 . 4) (t . 0))");
}

/// Auto-fill typing followed by undo -- the case three separate undo fixes
/// (ledger 97, 109 and 115) all ran through on 2026-08-14 without any of them
/// covering it.
///
/// Typing wraps the line as the user types; one `undo' must take back only the
/// last command group, leaving the text the earlier groups produced.  With the
/// macro-edge boundary lost, `undo' ran its "get rid of initial undo boundary"
/// `undo-more' against a real group instead of against a boundary
/// (lisp/simple.el:3509-3511) and so undid two groups -- here, the whole
/// buffer.
///
/// Measured on GNU Emacs -Q --batch:
/// `("aaaa bbbb cccc dddd\neeee " "aaaa bbbb cccc dddd e" 22)'.
#[test]
fn auto_fill_typing_then_undo_takes_back_only_the_last_command_group() {
    crate::test_utils::init_test_tracing();
    let result = undo_bootstrap_eval_one(&format!(
        r#"(progn
             {UNDO_TYPING_PRELUDE}
             (let ((buffer (neo-undo-pin-open 20)))
               (auto-fill-mode 1)
               (execute-kbd-macro (string-to-vector "aaaa bbbb cccc dddd eeee "))
               (let ((typed (buffer-substring-no-properties (point-min) (point-max))))
                 (execute-kbd-macro (kbd "C-/"))
                 (list typed
                       (buffer-substring-no-properties (point-min) (point-max))
                       (point)))))"#
    ));
    assert_eq!(
        result,
        "OK (\"aaaa bbbb cccc dddd\neeee \" \"aaaa bbbb cccc dddd e\" 22)"
    );
}

// ===========================================================================
// GC-time undo-list truncation
//
// GNU truncates undo lists from `garbage_collect' (`src/alloc.c:5797-5800'),
// which walks every live buffer through `compact_buffer'
// (`src/buffer.c:1854-1885') into `truncate_undo_list' (`src/undo.c:289-419').
// `Fundo_boundary' (`src/undo.c:251-282') never truncates.
//
// Every expected value below was measured by running the same fixture under
// GNU Emacs 31.0.90 `--batch'.
// ===========================================================================

use crate::emacs_core::eval::Context;

/// GROUPS one-entry undo groups in a buffer whose own `undo-limit' and
/// `undo-strong-limit' are LIMIT and STRONG.
fn undo_limit_fixture(eval: &mut Context, name: &str, limit: i64, strong: i64, groups: usize) {
    let src = format!(
        r#"(progn
             (set-buffer (get-buffer-create "{name}"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-limit)
             (make-local-variable 'undo-strong-limit)
             (setq undo-limit {limit})
             (setq undo-strong-limit {strong})
             (setq neo-undo-probe-i 0)
             (while (< neo-undo-probe-i {groups})
               (insert "hello")
               (undo-boundary)
               (setq neo-undo-probe-i (1+ neo-undo-probe-i))))"#
    );
    eval.eval_str(&src).expect("undo limit fixture");
}

fn printed_undo_list(eval: &mut Context, name: &str) -> String {
    let value = eval
        .eval_str(&format!(
            r#"(buffer-local-value 'buffer-undo-list (get-buffer "{name}"))"#
        ))
        .expect("read buffer-undo-list");
    crate::emacs_core::print::print_value(&value)
}

fn undo_list_length(eval: &mut Context, name: &str) -> i64 {
    let value = eval
        .eval_str(&format!(
            r#"(length (buffer-local-value 'buffer-undo-list (get-buffer "{name}")))"#
        ))
        .expect("length of buffer-undo-list");
    match value.kind() {
        ValueKind::Fixnum(n) => n,
        other => panic!("length returned {other:?}"),
    }
}

/// GNU truncates at GC, never at `undo-boundary'.
///
/// GNU Emacs -Q --batch, same fixture: before=21, after=2.
#[test]
fn undo_boundary_does_not_truncate_and_gc_does() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    undo_limit_fixture(&mut eval, "A", 1, 1, 10);

    // Ten `undo-boundary' calls with a 1-byte limit: GNU still has the whole
    // list, because `Fundo_boundary' does not truncate.
    assert_eq!(undo_list_length(&mut eval, "A"), 21);

    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "A"), 2);
    assert_eq!(printed_undo_list(&mut eval, "A"), "(nil (46 . 51))");
}

/// The buffer's OWN `undo-limit' governs, not the global default: GNU makes
/// the buffer current before reading them (`src/undo.c:301-304').
///
/// GNU Emacs -Q --batch, same fixture: A-len=2 B-len=21.
#[test]
fn gc_truncation_reads_each_buffers_own_undo_limit() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    undo_limit_fixture(&mut eval, "A", 1, 1, 10);
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "B"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (setq neo-undo-probe-i 0)
             (while (< neo-undo-probe-i 10)
               (insert "hello")
               (undo-boundary)
               (setq neo-undo-probe-i (1+ neo-undo-probe-i))))"#,
    )
    .expect("global-limit fixture");

    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "A"), 2);
    assert_eq!(undo_list_length(&mut eval, "B"), 21);
}

/// Truncation lands on a group edge, never inside a group.
///
/// GNU Emacs -Q --batch, same fixture:
///   after=10 (nil (46 . 51) nil (41 . 46) nil (36 . 41) nil (31 . 36) nil (26 . 31))
#[test]
fn gc_truncation_cuts_at_a_group_boundary() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    undo_limit_fixture(&mut eval, "B", 200, 1000, 10);

    eval.gc_collect_exact();

    assert_eq!(
        printed_undo_list(&mut eval, "B"),
        "(nil (46 . 51) nil (41 . 46) nil (36 . 41) nil (31 . 36) nil (26 . 31))"
    );
}

/// The most recent record survives however small the limits are: GNU scans
/// past the first group before it starts making truncation decisions
/// (`src/undo.c:323-347').
///
/// GNU Emacs -Q --batch, same fixture: before=9, after=9.
#[test]
fn gc_truncation_always_keeps_the_most_recent_record() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "C"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-limit)
             (make-local-variable 'undo-strong-limit)
             (setq undo-limit 1)
             (setq undo-strong-limit 1)
             (setq neo-undo-probe-i 0)
             (while (< neo-undo-probe-i 8)
               (insert "hello")
               (goto-char (point-min))
               (setq neo-undo-probe-i (1+ neo-undo-probe-i))))"#,
    )
    .expect("single-group fixture");
    assert_eq!(undo_list_length(&mut eval, "C"), 9);

    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "C"), 9);
}

/// A buffer not modified since the last compaction is skipped, so a list the
/// user installed by hand is left alone (GNU `compact_buffer''s
/// `BUF_COMPACT (buffer) != BUF_MODIFF (buffer)' guard, `src/buffer.c:1861').
///
/// GNU Emacs -Q --batch, same fixture:
///   after-first-gc=2, after-second-gc=6.
#[test]
fn gc_skips_a_buffer_unmodified_since_the_last_compaction() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    undo_limit_fixture(&mut eval, "D", 1, 1, 10);
    eval.gc_collect_exact();
    assert_eq!(undo_list_length(&mut eval, "D"), 2);

    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer "D"))
             (setq buffer-undo-list
                   (list nil (cons 1 2) nil (cons 2 3) nil (cons 3 4))))"#,
    )
    .expect("hand-installed undo list");
    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "D"), 6);
}

/// `undo-outer-limit' asks `undo-outer-limit-function' first, with the buffer
/// current and the measured size as the sole argument; a non-nil answer means
/// "handled" and GNU touches nothing else (`src/undo.c:349-369').
///
/// GNU Emacs -Q --batch, same fixture:
///   fn size=64 buf=E, after=2 ((1 . 6) (t . 0)).
#[test]
fn gc_consults_undo_outer_limit_function_with_the_buffer_current() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "E"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-outer-limit)
             (setq undo-outer-limit 1)
             (setq neo-undo-outer-calls nil)
             (setq undo-outer-limit-function
                   (lambda (size)
                     (setq neo-undo-outer-calls
                           (cons (cons size (buffer-name)) neo-undo-outer-calls))
                     t))
             (insert "hello"))"#,
    )
    .expect("outer-limit fixture");

    eval.gc_collect_exact();

    let calls = eval
        .eval_str("neo-undo-outer-calls")
        .expect("calls recorded");
    assert_eq!(
        crate::emacs_core::print::print_value(&calls),
        r#"((64 . "E"))"#
    );
    assert_eq!(printed_undo_list(&mut eval, "E"), "((1 . 6) (t . 0))");
}

/// A nil answer from `undo-outer-limit-function' falls through to the ordinary
/// `undo-limit' truncation (`src/undo.c:362-368').
///
/// GNU Emacs -Q --batch, same fixture:
///   fn-called-with=(64 "D") list-len-after=2.
#[test]
fn nil_from_undo_outer_limit_function_falls_through_to_normal_truncation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "D"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-outer-limit)
             (setq undo-outer-limit 1)
             (setq neo-undo-outer-size nil)
             (setq undo-outer-limit-function
                   (lambda (size) (setq neo-undo-outer-size size) nil))
             (insert "hello"))"#,
    )
    .expect("outer-limit fallthrough fixture");

    eval.gc_collect_exact();

    let size = eval.eval_str("neo-undo-outer-size").expect("size recorded");
    assert_eq!(size, Value::fixnum(64));
    assert_eq!(undo_list_length(&mut eval, "D"), 2);
}

/// `undo-outer-limit' alone does nothing: GNU only acts through the function
/// (`src/undo.c:356').
///
/// GNU Emacs -Q --batch, same fixture: after=2.
#[test]
fn undo_outer_limit_without_a_function_does_nothing() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "E"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-outer-limit)
             (setq undo-outer-limit 1)
             (setq undo-outer-limit-function nil)
             (insert "hello"))"#,
    )
    .expect("outer-limit-without-function fixture");

    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "E"), 2);
}

/// A buffer with undo turned off keeps `t': calling `truncate_undo_list' on
/// `t' would turn undo back on, which is exactly why GNU guards the call
/// (`src/buffer.c:1865-1870').
///
/// GNU Emacs -Q --batch, same fixture: after=t.
#[test]
fn gc_leaves_a_buffer_with_undo_disabled_alone() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "F"))
             (setq buffer-undo-list t)
             (insert "hello"))"#,
    )
    .expect("undo-disabled fixture");

    eval.gc_collect_exact();

    assert_eq!(printed_undo_list(&mut eval, "F"), "t");
}

/// Size limits lowered by `undo-outer-limit-function' before it answers nil
/// still apply: GNU reads `undo_limit' and `undo_strong_limit' during the walk
/// that follows the call, not before it (`src/undo.c:386-389').
///
/// GNU Emacs -Q --batch, same fixture: before=21, after=2.
#[test]
fn limits_lowered_by_undo_outer_limit_function_apply_to_the_same_pass() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "G"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-limit)
             (make-local-variable 'undo-strong-limit)
             (setq undo-limit 160000)
             (setq undo-strong-limit 240000)
             (make-local-variable 'undo-outer-limit)
             (setq undo-outer-limit 1)
             (setq undo-outer-limit-function
                   (lambda (_size)
                     (setq undo-limit 1)
                     (setq undo-strong-limit 1)
                     nil))
             (setq neo-undo-probe-i 0)
             (while (< neo-undo-probe-i 10)
               (insert "hello")
               (undo-boundary)
               (setq neo-undo-probe-i (1+ neo-undo-probe-i))))"#,
    )
    .expect("relowering fixture");
    assert_eq!(undo_list_length(&mut eval, "G"), 21);

    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "G"), 2);
}

/// ...and they are read from whatever buffer the function left current, since
/// GNU's are C globals that `set_buffer_internal' swaps.  A function that
/// lowers this buffer's limits but ends in another buffer therefore truncates
/// nothing: the other buffer's limits are what the walk sees.
///
/// GNU Emacs -Q --batch, same fixture: before=21, after=21.
#[test]
fn limits_are_reread_from_whatever_buffer_the_outer_limit_function_left_current() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (set-buffer (get-buffer-create "H"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (set-buffer (get-buffer-create "G"))
             (buffer-enable-undo)
             (setq buffer-undo-list nil)
             (make-local-variable 'undo-limit)
             (make-local-variable 'undo-strong-limit)
             (setq undo-limit 160000)
             (setq undo-strong-limit 240000)
             (make-local-variable 'undo-outer-limit)
             (setq undo-outer-limit 1)
             (setq undo-outer-limit-function
                   (lambda (_size)
                     (setq undo-limit 1)
                     (setq undo-strong-limit 1)
                     (set-buffer (get-buffer "H"))
                     nil))
             (setq neo-undo-probe-i 0)
             (while (< neo-undo-probe-i 10)
               (insert "hello")
               (undo-boundary)
               (setq neo-undo-probe-i (1+ neo-undo-probe-i))))"#,
    )
    .expect("buffer-switching fixture");
    assert_eq!(undo_list_length(&mut eval, "G"), 21);

    eval.gc_collect_exact();

    assert_eq!(undo_list_length(&mut eval, "G"), 21);
}

/// The exact domain of `undo-outer-limit': GNU's guard is
/// `INTEGERP (V) && (integer_to_intmax (V, &l) ? l < size : NILP (Fnatnump (V)))'
/// (`src/undo.c:352-355'), so a float is not a limit at all, a bignum too big
/// for `intmax_t' fires only when it is negative, and a positive one never
/// fires.
///
/// GNU Emacs -Q --batch, same fixture: only `neg-bignum' calls the function.
#[test]
fn undo_outer_limit_domain_matches_gnus_integer_guard() {
    crate::test_utils::init_test_tracing();
    for (name, limit, expected_calls) in [
        ("neg-bignum", "(- (* most-positive-fixnum 8))", 1),
        ("pos-bignum", "(* most-positive-fixnum 8)", 0),
        ("not-an-integer", "\"nope\"", 0),
        ("float", "1.0", 0),
        ("small", "1", 1),
    ] {
        let mut eval = Context::new();
        eval.eval_str(&format!(
            r#"(progn
                 (set-buffer (get-buffer-create "{name}"))
                 (buffer-enable-undo)
                 (setq buffer-undo-list nil)
                 (make-local-variable 'undo-outer-limit)
                 (setq undo-outer-limit {limit})
                 (setq neo-undo-outer-calls 0)
                 (setq undo-outer-limit-function
                       (lambda (_size)
                         (setq neo-undo-outer-calls (1+ neo-undo-outer-calls))
                         t))
                 (insert "hello"))"#
        ))
        .unwrap_or_else(|error| panic!("{name} fixture: {error:?}"));

        eval.gc_collect_exact();

        let calls = eval.eval_str("neo-undo-outer-calls").expect("call count");
        assert_eq!(
            calls,
            Value::fixnum(expected_calls),
            "undo-outer-limit = {limit}"
        );
    }
}

#[test]
fn undo_boundary_records_its_cause_only_on_the_path_gnu_takes() {
    // GNU's `Fundo_boundary' (src/undo.c:251-282) does
    // `Fset (Qundo_auto__last_boundary_cause, Qexplicit)' at :277 -- AFTER the
    // early return for a buffer whose `buffer-undo-list' is t (:258-259), and
    // immediately before it saves the point/buffer pair (:278-279).  So the
    // variable records "an explicit boundary happened here", and a buffer that
    // records nothing leaves it alone.
    //
    // `Freplace_buffer_contents' calls `Fundo_boundary' too
    // (src/editfns.c:2139), so it sets the cause by the same route.
    //
    // Expectations measured under GNU Emacs 31.0.90
    // (tmp/coord-boundary-cause-probe.el), not derived.
    crate::test_utils::init_test_tracing();
    // The runtime-startup context is required: `undo-auto--last-boundary-cause`
    // is a defvar-local in lisp/simple.el, and `with-temp-buffer` is a macro
    // from the same runtime, so a bare Context has neither.
    let results = crate::test_utils::runtime_startup_eval_all(
        r#"(setq undo-auto--last-boundary-cause nil)
           (with-temp-buffer
             (buffer-enable-undo) (insert "x") (undo-boundary)
             undo-auto--last-boundary-cause)
           (with-temp-buffer
             (setq buffer-undo-list t) (insert "x")
             (setq undo-auto--last-boundary-cause nil)
             (undo-boundary)
             undo-auto--last-boundary-cause)
           (with-temp-buffer
             (buffer-enable-undo) (insert "x")
             (setq undo-auto--last-boundary-cause 'something-else)
             (undo-boundary)
             undo-auto--last-boundary-cause)"#,
    );
    assert_eq!(
        results,
        vec![
            // The reset itself.
            "OK nil".to_string(),
            "OK explicit".to_string(),
            // The undo-disabled buffer returns before the assignment, so a
            // buffer that records nothing does not claim a boundary either.
            "OK nil".to_string(),
            // The assignment overwrites whatever was there.
            "OK explicit".to_string(),
        ]
    );
}
