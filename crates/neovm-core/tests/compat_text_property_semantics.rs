mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct TextPropertyCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_text_property_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping text property semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        TextPropertyCase {
            name: "string_overlapping_ranges_and_boundary_walk",
            form: r#"(let ((s (copy-sequence "abcdefghij")))
  (put-text-property 0 6 'face 'bold s)
  (put-text-property 3 8 'help-echo "tip" s)
  (list
   (text-properties-at 0 s)
   (text-properties-at 3 s)
   (text-properties-at 7 s)
   (next-property-change 0 s)
   (next-property-change 3 s)
   (next-property-change 6 s)
   (previous-property-change 8 s)
   (previous-property-change 3 s)))"#,
        },
        TextPropertyCase {
            name: "adjacent_identical_properties_merge",
            form: r#"(let ((s (copy-sequence "abcdef")))
  (put-text-property 0 3 'face 'bold s)
  (put-text-property 3 6 'face 'bold s)
  (list
   (next-property-change 0 s)
   (text-properties-at 2 s)
   (text-properties-at 4 s)))"#,
        },
        TextPropertyCase {
            name: "string_set_text_properties_nil_return_values",
            form: r#"(list
 (let ((s "abc"))
   (set-text-properties 0 3 nil s))
 (let ((s (copy-sequence "abc")))
   (put-text-property 0 1 'p t s)
   (set-text-properties 0 3 nil s))
 (let ((s "abc"))
   (set-text-properties 1 2 nil s))
 (let ((s "abc"))
   (set-text-properties 1 2 '(p t) s))
 (let ((s "abc"))
   (set-text-properties 1 1 '(p t) s))
 (let ((s (copy-sequence "abc")))
   (put-text-property 0 1 'p t s)
   (set-text-properties 0 3 nil s)
   (object-intervals s)))"#,
        },
        TextPropertyCase {
            name: "string_object_intervals_include_nil_gaps",
            form: r#"(list
 (let ((s (copy-sequence "abc")))
   (put-text-property 0 1 'p t s)
   (object-intervals s))
 (let ((s (copy-sequence "abc")))
   (put-text-property 1 2 'p t s)
   (object-intervals s))
 (let ((s (copy-sequence "abc")))
   (put-text-property 0 3 'p t s)
   (set-text-properties 1 2 nil s)
   (object-intervals s)))"#,
        },
        TextPropertyCase {
            name: "string_remove_text_properties_preserves_nil_intervals",
            form: r#"(list
 (let ((s (copy-sequence "ab")))
   (put-text-property 0 1 'p t s)
   (remove-text-properties 0 1 '(p nil) s)
   (list
    (object-intervals s)
    (next-property-change 0 s t)
    (next-property-change 0 s)
    (text-properties-at 0 s)
    (text-properties-at 1 s)))
 (let ((s (copy-sequence "abc")))
   (put-text-property 0 3 'p t s)
   (remove-text-properties 0 3 '(p nil) s)
   (list
    (object-intervals s)
    (next-property-change 0 s t)))
 (let ((s (copy-sequence "abc")))
   (put-text-property 0 3 'p t s)
   (set-text-properties 0 3 nil s)
   (list
    (object-intervals s)
    (next-property-change 0 s t))))"#,
        },
        TextPropertyCase {
            name: "text_property_search_default_properties_without_interval_tree",
            form: r#"(list
 (let ((default-text-properties '(fallback 7))
       (s "ab"))
   (list
    (get-text-property 0 'fallback s)
    (text-property-any 0 2 'fallback 7 s)
    (text-property-not-all 0 2 'fallback 7 s)
    (text-property-any 0 2 'fallback nil s)
    (text-property-not-all 0 2 'fallback nil s)))
 (let ((default-text-properties '(fallback 7)))
   (with-temp-buffer
     (insert "ab")
     (list
      (get-text-property 1 'fallback)
      (text-property-any 1 3 'fallback 7)
      (text-property-not-all 1 3 'fallback 7)
      (text-property-any 1 3 'fallback nil)
      (text-property-not-all 1 3 'fallback nil)))))"#,
        },
        TextPropertyCase {
            name: "text_property_search_empty_range_without_interval_tree",
            form: r#"(list
 (let ((s "ab"))
   (list
    (text-property-any 1 1 'fallback nil s)
    (text-property-not-all 1 1 'fallback 7 s)))
 (with-temp-buffer
   (insert "ab")
   (list
    (text-property-any 2 2 'fallback nil)
    (text-property-not-all 2 2 'fallback 7))))"#,
        },
        TextPropertyCase {
            name: "text_property_search_uses_eq_not_equal",
            form: r#"(list
 (let ((s (copy-sequence "ab"))
       (stored (list 1))
       (same-shape (list 1)))
   (put-text-property 0 1 'p stored s)
   (list
    (text-property-any 0 2 'p stored s)
    (text-property-any 0 2 'p same-shape s)
    (text-property-not-all 0 1 'p stored s)
    (text-property-not-all 0 1 'p same-shape s)))
 (with-temp-buffer
   (let ((stored (list 1))
         (same-shape (list 1)))
     (insert "ab")
     (put-text-property 1 2 'p stored)
     (list
      (text-property-any 1 3 'p stored)
      (text-property-any 1 3 'p same-shape)
      (text-property-not-all 1 2 'p stored)
      (text-property-not-all 1 2 'p same-shape)))))"#,
        },
        TextPropertyCase {
            name: "buffer_text_property_boundaries_use_buffer_positions",
            form: r#"(let ((buf (get-buffer-create " *compat-textprop-buffer*")))
  (unwind-protect
      (with-current-buffer buf
        (erase-buffer)
        (insert "abcdefgh")
        (put-text-property 1 5 'face 'bold)
        (put-text-property 5 7 'face 'italic)
        (list
         (text-properties-at 1)
         (text-properties-at 5)
         (next-property-change 1)
         (previous-property-change 7)))
    (kill-buffer buf)))"#,
        },
        TextPropertyCase {
            name: "plain_insert_splits_text_properties_around_new_text",
            form: r#"(with-temp-buffer
  (insert "abcd")
  (put-text-property 1 3 'foo 'bar)
  (goto-char 2)
  (insert "X")
  (list
   (buffer-string)
   (get-text-property 1 'foo)
   (get-text-property 2 'foo)
   (get-text-property 3 'foo)
   (get-text-property 4 'foo)
   (next-property-change 1)
   (next-property-change 2)
   (previous-property-change 4)))"#,
        },
        TextPropertyCase {
            name: "insert_and_inherit_merges_boundary_stickiness_per_property",
            form: r#"(with-temp-buffer
  (insert "ab")
  (put-text-property 1 2 'left-only 'l)
  (put-text-property 1 2 'carry 'left)
  (put-text-property 1 2 'rear-nonsticky '(carry right-only))
  (put-text-property 2 3 'carry 'right)
  (put-text-property 2 3 'right-only 'r)
  (put-text-property 2 3 'front-sticky '(carry right-only))
  (goto-char 2)
  (insert-and-inherit "X")
  (list
   (buffer-string)
   (text-properties-at 2)
   (text-properties-at 3)
   (next-property-change 1)
   (next-property-change 2)
   (previous-property-change 4)))"#,
        },
        TextPropertyCase {
            name: "insert_and_inherit_honors_text_property_default_nonsticky",
            form: r#"(let ((text-property-default-nonsticky '((carry . t) (right-only))))
  (with-temp-buffer
    (insert "ab")
    (put-text-property 1 2 'carry 'left)
    (put-text-property 1 2 'left-only 'l)
    (put-text-property 2 3 'right-only 'r)
    (goto-char 2)
    (insert-and-inherit "X")
    (list
     (buffer-string)
     (text-properties-at 2)
     (text-properties-at 3)
     (get-text-property 2 'carry)
     (get-text-property 2 'left-only)
     (get-text-property 2 'right-only))))"#,
        },
        TextPropertyCase {
            name: "char_property_lookup_uses_alias_category_and_defaults",
            form: r#"(let ((default-text-properties '(fallback 7))
      (char-property-alias-alist '((face font-lock-face))))
  (with-temp-buffer
    (insert "ab")
    (put-text-property 1 2 'font-lock-face 'keyword)
    (put-text-property 1 2 'category 'neo-cat)
    (put 'neo-cat 'cat-prop 'catv)
    (list
     (get-text-property 1 'face)
     (get-char-property 1 'face)
     (get-text-property 1 'cat-prop)
     (get-char-property 1 'cat-prop)
     (get-text-property 2 'fallback)
     (text-property-any 1 3 'face 'keyword)
     (text-property-any 1 3 'fallback 7)
     (text-property-not-all 1 3 'fallback 7)
     (next-single-property-change 1 'face)
     (previous-single-property-change 3 'face))))"#,
        },
        TextPropertyCase {
            name: "buffer_local_default_nonsticky_controls_insert_inherit",
            form: r#"(with-temp-buffer
  (setq-local text-property-default-nonsticky '((carry . t)))
  (insert "ab")
  (put-text-property 1 2 'carry 'left)
  (goto-char 2)
  (insert-and-inherit "X")
  (list
   (buffer-string)
   (get-text-property 2 'carry)
   (text-properties-at 2)
   (text-properties-at 3)))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "text property semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
