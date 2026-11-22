use super::*;
use std::time::Duration;

/// `current-column` reaches the scan only through `current_column_1`
/// (intervals present, `src/indent.c:365-370`); with `long-line-threshold`
/// nil the :348 approximation is off, so the fast path at :629 is what makes
/// the two buffers differ.  A property leaves GNU's interval tree in place
/// even after the property is removed, so the control uses a second buffer
/// that never had one.
#[test]
fn current_column_1_long_truncated_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (let ((prop (get-buffer-create \"*p640-c1-prop*\")) \
                 (plain (get-buffer-create \"*p640-c1-plain*\"))) \
             (dolist (b (list prop plain)) \
               (set-buffer b) (erase-buffer) \
               (insert (make-string 1000 ?a)) (insert \"\\t\") \
               (insert (make-string 200000 ?a)) (insert \"\\n\") \
               (setq truncate-lines t) (goto-char 30001) \
               (switch-to-buffer b) (redisplay t)) \
             (set-buffer prop) (put-text-property 1 2 'p640 t) \
             (setq long-line-threshold nil) \
             (switch-to-buffer plain)) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640C1",
        "(list (with-current-buffer \"*p640-c1-prop*\" (current-column)) \
                (with-current-buffer \"*p640-c1-plain*\" (current-column)))",
    );
    eprintln!("issue#640 current_column_1: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(30000 30007)",
        "GNU: properties branch approximates, the property-free buffer does not"
    );
    assert_eq!(neo_value, gnu_value, "current_column_1 must match GNU");
}

/// GNU's interval tree is tree PRESENCE, not "has a property":
/// `remove-text-properties` validates with `soft` and never discards the
/// root (`src/textprop.c:1607-1619`), so after the last property is removed
/// `buffer_intervals` is still non-NULL and `current-column` still takes
/// `current_column_1` (`src/indent.c:365-370`) and its long+truncated
/// shortcut.  Attested: GNU prints 30000.
#[test]
fn current_column_1_removed_property_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (switch-to-buffer (get-buffer-create \"*p640-c1-removed*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) (insert \"\\t\") \
           (insert (make-string 200000 ?a)) (insert \"\\n\") \
           (setq truncate-lines t) (goto-char 30001) \
           (redisplay t) \
           (put-text-property 1 2 'p640 t) \
           (remove-text-properties 1 2 '(p640 nil)) \
           (setq long-line-threshold nil) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640C1R", "(current-column)");
    eprintln!("issue#640 removed-property current_column_1: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "30000",
        "GNU keeps the interval tree after remove-text-properties"
    );
    assert_eq!(
        neo_value, gnu_value,
        "the removed-property row must match GNU"
    );
}

/// GNU creates the interval tree when text carrying properties is inserted
/// (`copy_intervals` in `insert_1_both`, src/insdel.c), so `buffer_intervals`
/// is non-NULL even though the buffer never had an explicit property store.
/// `current-column` therefore takes `current_column_1` (src/indent.c:365-370)
/// and skips the invisible text; removing the property keeps the tree.
/// Attested GNU: (2 (nil 5)).
#[test]
fn current_column_after_inserting_propertized_text_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640PROP",
        "(with-temp-buffer \
           (insert (propertize \"abc\" 'invisible t) \"de\") \
           (list (current-column) \
                 (progn (remove-text-properties (point-min) (point-max) '(invisible nil)) \
                        (list (text-properties-at 1) (current-column)))))",
    );
    eprintln!("issue#640 inserted propertized text: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(2 (nil 5))",
        "GNU skips the inserted invisible text and keeps the tree after removal"
    );
    assert_eq!(
        neo_value, gnu_value,
        "inserted property runs must report GNU's interval tree"
    );
}

/// GNU's `current_column` line start is `find_newline (PT, ..., BEGV)`
/// (`src/indent.c:344-345`): in a narrowed buffer the walk stops at BEGV, so
/// the approximation returns PT - BEGV.  Neo's `bol_emacs_byte` must use the
/// accessible start the same way; using the buffer's first line would return
/// 399999 instead of the attested 299999.
#[test]
fn current_column_stops_at_begv_in_a_narrowed_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (switch-to-buffer (get-buffer-create \"*p640-narrow*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) (insert \"\\t\") \
           (insert (make-string 1200000 ?a)) \
           (setq truncate-lines t) \
           (narrow-to-region 100001 600000) \
           (goto-char 400000) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640NB", "(current-column)");
    eprintln!("issue#640 narrowed current-column: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "299999",
        "GNU stops the line start at BEGV (400000 - 100001)"
    );
    assert_eq!(neo_value, gnu_value, "BEGV must bound Neo's line start too");
}
