use crate::emacs_core::eval::Context;

#[test]
fn absent_plist_removal_preserves_string_intervals() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((s (copy-sequence "abcdef")))
                 (put-text-property 0 6 'keep t s)
                 (list (remove-text-properties 1 5 '(absent nil) s)
                       (object-intervals s)
                       (next-property-change 0 s t)
                       (next-property-change 1 s 4)))"#,
        )
        .expect("remove-text-properties");
    assert_eq!(format!("{result}"), "(nil ((0 6 (keep t))) 6 4)");
}

#[test]
fn absent_name_list_removal_preserves_string_intervals() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((s (copy-sequence "aé🙂b")))
                 (put-text-property 0 4 'keep t s)
                 (let ((before (object-intervals s)))
                   (list (remove-list-of-text-properties 1 3 '(absent) s)
                         (equal before (object-intervals s)))))"#,
        )
        .expect("remove-list-of-text-properties");
    assert_eq!(format!("{result}"), "(nil t)");
}

#[test]
fn removal_keeps_absent_endpoint_intervals_unsplit() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((s (copy-sequence "abcdef")))
                 (put-text-property 0 6 'keep t s)
                 (put-text-property 2 4 'drop nil s)
                 (list (remove-text-properties 1 5 '(absent nil drop nil) s)
                       (object-intervals s)))"#,
        )
        .expect("remove-text-properties with absent endpoints");
    assert_eq!(
        format!("{result}"),
        "(t ((0 2 (keep t)) (2 4 (keep t)) (4 6 (keep t))))"
    );
}

#[test]
fn buffer_removal_keeps_absent_endpoint_intervals_unsplit() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(progn
                 (insert "abcdef")
                 (put-text-property 1 7 'keep t)
                 (put-text-property 3 5 'drop nil)
                 (list (remove-list-of-text-properties 2 6 '(absent drop))
                       (object-intervals (current-buffer))))"#,
        )
        .expect("buffer remove-list-of-text-properties");
    assert_eq!(
        format!("{result}"),
        "(t ((0 2 (keep t)) (2 4 (keep t)) (4 6 (keep t))))"
    );
}

#[test]
fn removal_checks_property_names_added_through_the_live_plist() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((s (propertize "abcdef" 'keep t)))
                 (setcdr (cdr (text-properties-at 0 s)) '(added t))
                 (list (remove-text-properties 1 5 '(added nil) s)
                       (object-intervals s)))"#,
        )
        .expect("remove-text-properties through a mutated plist");
    assert_eq!(
        format!("{result}"),
        "(t ((0 1 (keep t added t)) (1 5 (keep t)) (5 6 (keep t added t))))"
    );
}
