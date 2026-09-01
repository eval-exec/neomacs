use crate::emacs_core::format_eval_result;
use crate::emacs_core::value::Value;

/// GNU `Fget_truename_buffer` (src/buffer.c:524-539) returns the live buffer
/// whose `buffer-file-truename` is `string-equal` to FILENAME.  It used to be
/// a stub returning nil here, which silently disabled the supersession check
/// in `lock_file` (filelock.c:603).
#[test]
fn get_truename_buffer_finds_the_visiting_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_file_truename(current, Value::string("/work/note.txt"))
        .expect("set buffer-file-truename");

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(eq (get-truename-buffer "/work/note.txt")
                                                 (current-buffer))"#
        )),
        "OK t",
    );
    assert_eq!(
        format_eval_result(&eval.eval_str(r#"(get-truename-buffer "/work/other.txt")"#)),
        "OK nil",
        "GNU compares truenames literally, with no expansion or fallback"
    );
}

/// GNU's `general_insert_function` (src/editfns.c:1307-1345) converts and
/// inserts one argument at a time, so `wrong_type_argument` for a later
/// argument leaves every preceding argument already in the buffer.  Neomacs
/// used to validate the whole argument vector before touching the buffer, so a
/// package that passed a valid prefix plus a bad value inserted nothing.
///
/// Verified expectations come from running the same forms under GNU Emacs:
///   (insert "ab" '(1) "c")  => buffer "ab",    point 3
///   (insert "pick " '("x") "\n") => buffer "pick ", point 6, modified t
#[test]
fn insert_keeps_the_arguments_it_already_inserted_before_signalling_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(progn
                  (erase-buffer)
                  (list (condition-case e (insert "ab" '(1) "c") (error e))
                        (buffer-string)
                        (point)))"#
        )),
        r#"OK ((wrong-type-argument char-or-string-p (1)) "ab" 3)"#,
    );

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(progn
                  (erase-buffer)
                  (list (condition-case e (insert "pick " '("x") "\n") (error e))
                        (buffer-string)
                        (point)
                        (buffer-modified-p)))"#
        )),
        r#"OK ((wrong-type-argument char-or-string-p ("x")) "pick " 6 t)"#,
    );
}

/// GNU signals the same way for every member of the `general_insert_function`
/// family, and each variant still inserts its valid prefix first.
#[test]
fn insert_variants_all_insert_their_valid_prefix_before_signalling_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    for name in [
        "insert",
        "insert-before-markers",
        "insert-and-inherit",
        "insert-before-markers-and-inherit",
    ] {
        assert_eq!(
            format_eval_result(&eval.eval_str(&format!(
                r#"(progn
                      (erase-buffer)
                      (list (condition-case e ({name} "ab" 67 '(1)) (error e))
                            (buffer-string)))"#
            ))),
            r#"OK ((wrong-type-argument char-or-string-p (1)) "abC")"#,
            "{name} must insert its valid prefix before signalling"
        );
    }
}

/// GNU never expands or canonicalizes either side: `find-file` has already
/// stored the truename, so a relative or unexpanded FILENAME does not match.
#[test]
fn get_truename_buffer_does_not_expand_its_argument_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_file_truename(current, Value::string("/work/note.txt"))
        .expect("set buffer-file-truename");

    assert_eq!(
        format_eval_result(&eval.eval_str(r#"(get-truename-buffer "note.txt")"#)),
        "OK nil",
    );
}

/// GNU's `buffer-enable-undo` (src/buffer.c:1845-1847) resets the list ONLY
/// when undo is actually off:
///
/// ```c
///   if (EQ (BVAR (XBUFFER (real_buffer), undo_list), Qt))
///     bset_undo_list (XBUFFER (real_buffer), Qnil);
/// ```
///
/// Ours reset unconditionally, which destroyed an existing history.  The
/// damage is worst through an indirect buffer, because an indirect buffer
/// shares its base's undo list (`make_indirect_buffer`, src/buffer.c:894, plus
/// the per-switch resync in `set_buffer_internal_2`, src/buffer.c:2352-2367):
/// enabling undo in the indirect buffer wiped the BASE buffer's history.
///
/// The expected values were taken by running this exact form under GNU Emacs
/// 31.0.90 `-Q --batch`.  Each step is snapshotted with `prin1-to-string` at
/// capture time on purpose -- `record_insert` coalesces by mutating the head
/// cons in place (src/undo.c:109), so collecting the live lists and printing
/// them at the end reports the FINAL state for every earlier step.
#[test]
fn buffer_enable_undo_keeps_an_existing_list_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let* ((base (get-buffer-create "b2")) s1 s2 s3 s4)
                 (set-buffer base) (buffer-enable-undo) (insert "hello")
                 (setq s1 (prin1-to-string buffer-undo-list))
                 (let ((ind (make-indirect-buffer base "i2")))
                   (setq s2 (prin1-to-string buffer-undo-list))
                   (set-buffer ind) (buffer-enable-undo)
                   (set-buffer base) (setq s3 (prin1-to-string buffer-undo-list))
                   (set-buffer ind) (insert "Y")
                   (set-buffer base) (setq s4 (prin1-to-string buffer-undo-list)))
                 (list s1 s2 s3 s4))"#
        )),
        r#"OK ("((1 . 6) (t . 0))" "((1 . 6) (t . 0))" "((1 . 6) (t . 0))" "((1 . 7) (t . 0))")"#,
    );
}

/// The same clobber without any indirection: `buffer-enable-undo` on a buffer
/// that already has a history is a no-op in GNU.
#[test]
fn buffer_enable_undo_is_a_noop_when_undo_is_already_on_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((before nil))
                 (set-buffer (get-buffer-create "b4"))
                 (setq buffer-undo-list nil)
                 (insert "abc")
                 (setq before (prin1-to-string buffer-undo-list))
                 (buffer-enable-undo)
                 (list before (prin1-to-string buffer-undo-list)))"#
        )),
        r#"OK ("((1 . 4) (t . 0))" "((1 . 4) (t . 0))")"#,
    );
}

/// `buffer-enable-undo` still has to turn undo back ON when it is off, and it
/// does so for the whole shared chain: the indirect buffer and its base read
/// one list, so disabling undo through the indirect buffer leaves the BASE at
/// `t` and re-enabling through it clears the base back to nil.  Both halves
/// confirmed under GNU 31.0.90.
#[test]
fn buffer_enable_undo_still_clears_a_disabled_list_through_an_indirect_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let* ((base (get-buffer-create "b5")) disabled)
                 (set-buffer base) (buffer-enable-undo) (insert "hello")
                 (let ((ind (make-indirect-buffer base "i5")))
                   ;; `buffer-disable-undo' is Lisp in GNU (lisp/simple.el:3591)
                   ;; and has no subr here, so a bare evaluator writes its
                   ;; body: (setq buffer-undo-list t).  DIVERGENCES.md 150.
                   (set-buffer ind) (setq buffer-undo-list t)
                   (set-buffer base) (setq disabled (prin1-to-string buffer-undo-list))
                   (set-buffer ind) (buffer-enable-undo)
                   (set-buffer base)
                   (list disabled (prin1-to-string buffer-undo-list))))"#
        )),
        r#"OK ("t" "nil")"#,
    );
}

/// GNU's `record_point` guards the point entry with THREE conditions, and the
/// third is about the BUFFER (src/undo.c:73-75):
///
/// ```c
///   if (at_boundary
///       && point_before_last_command_or_undo != beg
///       && buffer_before_last_command_or_undo == current_buffer )
/// ```
///
/// `point_before_last_command_or_undo` and `buffer_before_last_command_or_undo`
/// are written together at both of their assignment sites -- the command loop
/// (src/keyboard.c:1536-1537) and `Fundo_boundary` (src/undo.c:278-279) -- so
/// a saved point is only usable in the buffer it was saved in.
///
/// We stored only the point, and stored it in the SHARED undo state, so the
/// point saved while the INDIRECT buffer was current was consumed by the next
/// edit in the BASE buffer and consed a point entry GNU does not record.
/// Confirmed under GNU 31.0.90 `-Q --batch`: `((6 . 7) nil (1 . 7) (t . 0))`,
/// where we produced `((6 . 7) 7 nil (1 . 7) (t . 0))`.
#[test]
fn a_point_saved_in_one_buffer_is_not_recorded_in_another_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let* ((base (get-buffer-create "b9")))
                 (set-buffer base) (buffer-enable-undo) (insert "hello") (goto-char 6)
                 (let ((ind (make-indirect-buffer base "i9")))
                   (set-buffer ind) (goto-char (point-max)) (insert "Y")
                   (undo-boundary)
                   (set-buffer base)
                   (insert "Z")
                   (list (point) (prin1-to-string buffer-undo-list))))"#
        )),
        r#"OK (7 "((6 . 7) nil (1 . 7) (t . 0))")"#,
    );
}

/// GNU reads an inserted string's BYTES and INTERVALS *after*
/// `before-change-functions`, not before.  `insert_from_string_1`
/// (src/insdel.c:1020-1098) takes `nchars`/`nbytes` from the caller --
/// `general_insert_function` passes `SCHARS (val)` / `SBYTES (val)`
/// (src/editfns.c:1337-1340) -- but `prepare_to_modify_buffer (PT, PT, NULL)`
/// (src/insdel.c:1043), which is what runs the hook, sits BETWEEN that
/// snapshot and both `copy_text (SDATA (string) + pos_byte, ...)`
/// (src/insdel.c:1053) and `intervals = string_intervals (string)`
/// (src/insdel.c:1093).
///
/// This is sound in GNU for a reason worth naming, because it is what makes
/// the same shape sound here: `Faset` on a string (src/data.c:2658-2681) is
/// strictly length-preserving in BOTH chars and bytes -- multibyte strings
/// take ASCII-for-ASCII in place, unibyte strings take `SSET` -- so no Lisp
/// operation can invalidate the pre-hook `nchars`/`nbytes`.  Only the bytes
/// behind the pointer and the interval tree can move, and GNU re-reads both
/// through `string` (a rooted `Lisp_Object`) at the point of use.
///
/// We materialized the whole `InsertPiece` -- converted bytes AND a clone of
/// the text-property table -- before signalling, so a hook that mutated or
/// propertized the string it was about to see inserted was invisible.
///
/// Verified against GNU Emacs 31.0.90; see DIVERGENCES.md 163 §10 and 164.
#[test]
fn insert_reads_the_string_after_before_change_functions_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (copy-sequence "abcdefgh")))
                 (setq before-change-functions
                       (list (lambda (&rest _) (aset s 0 ?Z))))
                 (insert s)
                 (buffer-string))"#
        )),
        r#"OK "Zbcdefgh""#,
        "GNU's copy_text runs after prepare_to_modify_buffer"
    );

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (copy-sequence "abcdefgh")))
                 (setq before-change-functions
                       (list (lambda (&rest _) (aset s 1 ?Y))))
                 (insert-before-markers s)
                 (buffer-string))"#
        )),
        r#"OK "aYcdefgh""#,
        "insert-before-markers shares insert_from_string_1"
    );

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (copy-sequence "abcdefgh")))
                 (setq before-change-functions
                       (list (lambda (&rest _)
                                     (put-text-property 0 3 'face 'bold s))))
                 (insert s)
                 (get-text-property 1 'face))"#
        )),
        "OK bold",
        "GNU grafts string_intervals (string) read after the hook"
    );

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (copy-sequence "abcdefgh")))
                 (insert s)
                 (buffer-string))"#
        )),
        r#"OK "abcdefgh""#,
        "control: no hook, unchanged"
    );
}

/// The other three doors into GNU's `general_insert_function`
/// (src/editfns.c:1373-1424) reach the same `insert_from_string_1`, so the
/// late read is not a property of `insert` alone.  Pinned separately from the
/// `insert` case so a regression names which door it came through.
///
/// The `set-text-properties` row is the inverse of the `put-text-property`
/// one and is the reason the fix cannot be "merge the hook's properties into
/// the snapshot": GNU takes `string_intervals (string)` wholesale after the
/// hook (src/insdel.c:1093), so properties the hook REMOVED are gone too.
///
/// Verified against GNU Emacs 31.0.90.
#[test]
fn every_insert_door_reads_the_string_after_the_change_hook_like_gnu() {
    crate::test_utils::init_test_tracing();

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (copy-sequence "abcdefgh")))
                 (setq before-change-functions
                       (list (lambda (&rest _) (aset s 0 ?Z))))
                 (insert-and-inherit s)
                 (buffer-string))"#
        )),
        r#"OK "Zbcdefgh""#,
        "insert-and-inherit"
    );

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (copy-sequence "abcdefgh")))
                 (setq before-change-functions
                       (list (lambda (&rest _) (aset s 2 ?W))))
                 (insert-before-markers-and-inherit s)
                 (buffer-string))"#
        )),
        r#"OK "abWdefgh""#,
        "insert-before-markers-and-inherit"
    );

    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s (propertize (copy-sequence "abcdefgh") 'face 'italic)))
                 (setq before-change-functions
                       (list (lambda (&rest _) (set-text-properties 0 8 nil s))))
                 (insert s)
                 (get-text-property 1 'face))"#
        )),
        "OK nil",
        "a hook that strips properties is observed too: GNU reads the whole \
         interval tree after the hook, it does not merge"
    );

    // The loop shape: only the argument the hook mutates changes, and the
    // hook fires once per argument, so argument 1 is already in the buffer
    // when the mutation lands.  GNU: "aaaZbb".
    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((s1 (copy-sequence "aaa")) (s2 (copy-sequence "bbb")))
                 (setq before-change-functions
                       (list (lambda (&rest _) (aset s2 0 ?Z))))
                 (insert s1 s2)
                 (buffer-string))"#
        )),
        r#"OK "aaaZbb""#,
        "one before-change signal per argument, mutation seen by argument 2"
    );

    // A character argument has nothing a hook can mutate, and GNU converts it
    // eagerly (`CHAR_STRING`, src/editfns.c:1327).  Control for the split.
    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(progn (setq before-change-functions (list (lambda (&rest _) nil)))
                      (insert ?a ?b ?c)
                      (buffer-string))"#
        )),
        r#"OK "abc""#,
        "character arm unaffected"
    );

    // An empty string returns before `prepare_to_modify_buffer`, so it runs
    // no hook at all (`insert_from_string`, src/insdel.c:986-987).  GNU: (1 "a").
    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let ((n 0))
                 (setq before-change-functions
                       (list (lambda (&rest _) (setq n (1+ n)))))
                 (insert "")
                 (insert "a")
                 (list n (buffer-string)))"#
        )),
        r#"OK (1 "a")"#,
        "empty argument signals nothing"
    );
}
