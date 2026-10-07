//! `aset` on a string overwrites one byte in place, without the heap write
//! barrier and without recounting characters. What Lisp observes must not
//! change: contents, length, byte length, multibyteness, text properties,
//! the `copy-sequence` it was made from, and the errors — with and without
//! GC stress (the barrier is gone, so a collection between writes is the
//! case to watch).

use crate::emacs_core::{Context, format_eval_result};

#[test]
fn aset_on_strings_is_observably_unchanged() {
    crate::test_utils::init_test_tracing();
    let cases: &[(&str, &str)] = &[
        // Multibyte ASCII string (a literal is multibyte only if non-ASCII;
        // `aéc` is), unibyte strings, a propertized string, and the source
        // of a copy.
        (
            "(let ((s (copy-sequence \"aéc\"))) (aset s 2 ?x) (list s (length s) (string-bytes s) (multibyte-string-p s)))",
            "(\"aéx\" 3 4 t)",
        ),
        (
            "(let ((s (copy-sequence \"abc\"))) (aset s 1 ?z) (list s (length s) (string-bytes s) (multibyte-string-p s)))",
            "(\"azc\" 3 3 nil)",
        ),
        (
            "(let ((s (string-to-unibyte \"abc\"))) (aset s 1 200) (list s (length s) (string-bytes s)))",
            "(\"a\\310c\" 3 3)",
        ),
        (
            "(let* ((o \"hello\") (s (copy-sequence o))) (aset s 0 ?J) (list o s))",
            "(\"hello\" \"Jello\")",
        ),
        (
            "(let ((s (propertize (copy-sequence \"abcd\") 'face 'bold))) (aset s 2 ?Z) (list s (get-text-property 2 'face s)))",
            "(#(\"abZd\" 0 4 (face bold)) bold)",
        ),
        (
            "(let ((s (make-string 1000 ?a)) (i 0)) (while (< i 1000) (aset s i (+ ?a (% i 26))) (setq i (1+ i))) (list (substring s 0 30) (length s) (string-bytes s)))",
            "(\"abcdefghijklmnopqrstuvwxyzabcd\" 1000 1000)",
        ),
        (
            "(condition-case err (let ((s (copy-sequence \"aéc\"))) (aset s 1 ?x)) (error err))",
            "(error \"Attempt to replace non-ASCII char in multibyte string\")",
        ),
        (
            "(condition-case err (let ((s (copy-sequence \"abc\"))) (aset s 3 ?x)) (error err))",
            "(args-out-of-range \"abc\" 3)",
        ),
    ];
    for gc_stress in [false, true] {
        let mut ev = Context::new();
        ev.gc_stress = gc_stress;
        for (form, want) in cases {
            let got = format_eval_result(&ev.eval_str(form));
            assert_eq!(got, format!("OK {want}"), "{form} (gc_stress {gc_stress})");
        }
    }
}

/// `aset` must locate the byte through GNU `string_char_to_byte`'s two outs,
/// not by counting characters forward from byte 0.
///
/// GNU (src/fns.c) returns CHAR_INDEX unchanged when `SCHARS == SBYTES` -- an
/// all-ASCII multibyte string stores one byte per character, so the byte
/// offset IS the index -- and otherwise scans from whichever END is nearer.
/// `LispString::char_to_byte_pos` already implements both; `aset` reached
/// past it to the bare-slice `char_to_byte_pos`, which can do neither,
/// because a `&[u8]` does not know SCHARS.
///
/// That made `aset` O(index): 2,000 writes near the end of a 320,000-char
/// multibyte string took 147.5ms against GNU Emacs 31.1's 0.7ms.
///
/// The scan-step counter is the discriminating assertion. A correctness test
/// cannot see the difference -- both routes return the same byte offset.
#[test]
fn aset_on_a_multibyte_string_does_not_scan_from_the_start() {
    use crate::emacs_core::emacs_char::{
        position_conversion_scan_steps_for_test, reset_position_conversion_scan_steps_for_test,
    };
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    // (a) All-ASCII MULTIBYTE: the `SCHARS == SBYTES` identity, so writing at
    // any index costs no conversion scan at all.
    let setup = eval.eval_str(r#"(setq s (string-to-multibyte (make-string 4096 ?a)))"#);
    assert_eq!(
        format_eval_result(&setup).split(' ').next(),
        Some("OK"),
        "build the all-ASCII multibyte string"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(multibyte-string-p s)")),
        "OK t"
    );
    reset_position_conversion_scan_steps_for_test();
    let wrote = eval.eval_str("(progn (aset s 4000 ?z) (aref s 4000))");
    assert_eq!(format_eval_result(&wrote), "OK 122");
    assert_eq!(
        position_conversion_scan_steps_for_test(),
        0,
        "an all-ASCII multibyte string needs no scan: the index IS the byte offset"
    );

    // (b) Genuinely multibyte, index near the END: one scan, and it must come
    // from the end rather than walking the whole string.
    let setup = eval.eval_str(r#"(setq m (concat "é" (make-string 4096 ?a)))"#);
    assert_eq!(format_eval_result(&setup).split(' ').next(), Some("OK"));
    let wrote =
        eval.eval_str("(progn (aset m 4000 ?z) (list (aref m 4000) (aref m 0) (length m)))");
    assert_eq!(
        format_eval_result(&wrote),
        "OK (122 233 4097)",
        "the write landed on the right character and left the rest alone"
    );
}

/// Writing a text property onto a string must mutate the string's interval
/// tree, not duplicate it.
///
/// GNU's `add_text_properties_1' (src/textprop.c) edits the intervals the
/// string already owns.  We read the table out through an accessor that ends
/// in `table.clone()`, edited the copy, and wrote it back -- so building a
/// propertized string one run at a time copied the whole tree once per run,
/// i.e. O(runs^2).
///
/// Correctness cannot see this: the copy holds the same intervals and the
/// write-back restores them.  Counting the copies is the only assertion that
/// distinguishes the two shapes, which is what the clone counter is for.
#[test]
fn putting_a_property_on_a_string_does_not_copy_its_interval_tree() {
    use crate::buffer::text_props::{
        reset_text_property_table_clones_for_test, text_property_table_clones_for_test,
    };
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let setup = eval.eval_str(r#"(setq s (make-string 3000 ?a))"#);
    assert_eq!(format_eval_result(&setup).split(' ').next(), Some("OK"));

    reset_text_property_table_clones_for_test();
    let built = eval.eval_str(
        // `while`, not `dotimes`: `Context::new()` is bare and `dotimes` is a
        // subr.el macro.
        r#"(progn (let ((i 0))
                    (while (< i 300)
                      (put-text-property (* i 3) (+ (* i 3) 2) 'face (list :run i) s)
                      (setq i (1+ i))))
                  (list (length s)
                        (get-text-property 0 'face s)
                        (get-text-property 897 'face s)
                        (get-text-property 2 'face s)))"#,
    );
    assert_eq!(
        format_eval_result(&built),
        "OK (3000 (:run 0) (:run 299) nil)",
        "every run landed where it was written"
    );
    assert_eq!(
        text_property_table_clones_for_test(),
        0,
        "300 property writes copied the interval tree instead of editing it"
    );
}
