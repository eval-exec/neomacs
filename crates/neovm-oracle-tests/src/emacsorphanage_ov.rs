//! Oracle parity tests for the `ov` overlay library (emacsorphanage/ov).
//!
//! `ov.el` (https://github.com/emacsorphanage/ov) is a real-world overlay
//! manipulation package. It exercises the overlay subsystem — overlay
//! creation/properties, `overlays-at`/`overlays-in` and priority ordering,
//! before/after-string, buffer-locality, and recentering — which is one of the
//! core surfaces being rewritten in Rust. These probes load the package from an
//! external checkout (the org mirror cloned under `EMACSORPHANAGE_ROOT`) and
//! compare GNU Emacs and Neomacs on *observable data* (lengths, positions,
//! property values, strings, counts) — never on overlay/buffer identity.
//!
//! Workflow:
//!
//! - Bake/update GNU expectations with
//!   `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1 cargo nextest run -p neovm-oracle-tests -E 'test(emacsorphanage_ov)'`.
//! - Verify Neomacs against the live GNU oracle with
//!   `NEOVM_ORACLE_MODE=verify cargo nextest run -p neovm-oracle-tests -E 'test(emacsorphanage_ov)'`.
//! - Run fast snapshot-mode CI with
//!   `cargo nextest run -p neovm-oracle-tests -E 'test(emacsorphanage_ov)'`.

use std::path::PathBuf;

use super::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Root of the emacsorphanage org mirror. Override with `EMACSORPHANAGE_ROOT`.
fn emacsorphanage_root() -> PathBuf {
    if let Ok(root) = std::env::var("EMACSORPHANAGE_ROOT") {
        return PathBuf::from(root);
    }
    if let Ok(home) = std::env::var("HOME") {
        let candidate = PathBuf::from(home)
            .join("Projects")
            .join("github.com")
            .join("emacsorphanage");
        if candidate.is_dir() {
            return candidate;
        }
    }
    // Fall back to a sibling checkout; the tests skip if it is absent.
    PathBuf::from("../emacsorphanage")
}

fn ov_dir() -> PathBuf {
    emacsorphanage_root().join("ov")
}

fn ov_el_present() -> bool {
    ov_dir().join("ov.el").is_file()
}

/// Skip gracefully when the package corpus is not checked out. This keeps the
/// suite green on machines/CI without the org mirror while exercising real
/// divergences wherever the corpus is present.
// The library artifact strips `#[test]` functions, so their macro invocations
// are absent there even though the test artifacts use this macro.
#[allow(unused_macros)]
macro_rules! return_if_corpus_missing {
    () => {
        if !ov_el_present() {
            tracing::info!(
                "skipping {}:{}: set EMACSORPHANAGE_ROOT or clone emacsorphanage/ov",
                module_path!(),
                line!()
            );
            return;
        }
    };
}

// --- load smoke test --------------------------------------------------------

#[test]
fn div_ov_loads_cleanly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK ov""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        "(require 'ov)",
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- create + property readback --------------------------------------------

#[test]
fn div_ov_create_with_properties_reads_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK (5 1 6 bold 5 \"hello\")""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "hello world")
    (let ((o (ov 1 6 'face 'bold 'priority 5)))
      (list (ov-length o)
            (overlay-start o)
            (overlay-end o)
            (overlay-get o 'face)
            (overlay-get o 'priority)
            (ov-string o)))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- ov-match: create overlays over literal matches ------------------------

#[test]
fn div_ov_match_creates_overlays_per_occurrence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect =
        expect_test::expect![[r#""OK (3 ((1 4 \"foo\") (9 12 \"foo\") (17 20 \"foo\")))""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "foo bar foo baz foo")
    (let ((ovs (ov-match "foo")))
      (list (length ovs)
            (mapcar (lambda (o) (list (overlay-start o) (overlay-end o) (ov-string o)))
                    (nreverse ovs))))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- ov-regexp: create overlays over regexp matches ------------------------

#[test]
fn div_ov_regexp_creates_overlays_for_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK (2 ((1 4 \"abc\") (8 11 \"axc\")))""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "abc xx axc bc axyzc")
    (let ((ovs (ov-regexp "a.c")))
      (list (length ovs)
            (mapcar (lambda (o) (list (overlay-start o) (overlay-end o) (ov-string o)))
                    (nreverse ovs))))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- ov-in / ov-all: filtering overlays by property ------------------------

#[test]
fn div_ov_in_filters_by_property_and_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK (2 1 1 2 2)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "abcdefghij")
    (ov 1 4 'face 'bold)
    (ov 5 8 'face 'italic 'priority 10)
    (list (length (ov-in 'face))
          (length (ov-in 'face 'italic))
          (length (ov-in 'face 'bold))
          (length (ov-in 1 8))
          (length (ov-all)))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- overlays-at ordering by priority --------------------------------------

#[test]
fn div_ov_priority_orders_overlays_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    // overlays-at returns overlays sorted by priority (highest first).
    let expect = expect_test::expect![[r#""OK (1 9 5)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "xxxxxxxxxx")
    (ov 2 5 'priority 1 'face 'a)
    (ov 3 6 'priority 9 'face 'b)
    (ov 4 7 'priority 5 'face 'c)
    (mapcar (lambda (o) (overlay-get o 'priority)) (overlays-at 4))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- ov-clear: removing overlays -------------------------------------------

#[test]
fn div_ov_clear_removes_all_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK (2 0)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "hello world")
    (ov-match "o")
    (let ((before (length (ov-all))))
      (ov-clear)
      (list before (length (ov-all))))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- before-string / after-string ------------------------------------------

#[test]
fn div_ov_before_after_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK (\"[\" \"]\")""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "ab")
    (let ((o (ov 1 2 'before-string "[" 'after-string "]")))
      (list (overlay-get o 'before-string) (overlay-get o 'after-string)))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- ov-line: overlay spanning a single line -------------------------------

#[test]
fn div_ov_line_spans_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    let expect = expect_test::expect![[r#""OK (7 13 \"line2\\n\")""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "line1\nline2\nline3")
    (goto-char 8)
    (let ((o (ov-line)))
      (list (overlay-start o) (overlay-end o) (ov-string o)))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}

// --- editing inside an overlay shifts the end ------------------------------

#[test]
fn div_ov_edit_inside_overlay_resizes_span() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    return_if_corpus_missing!();

    // ov sets front-advance=t, rear-advance=nil by default. Inserting at the
    // rear boundary (position 6) therefore lands *outside* the overlay: it
    // stays 1..6, length 5, string "hello". A reimplementation must match this
    // stickiness semantics exactly.
    let expect = expect_test::expect![[r#""OK (5 1 6 \"hello\")""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(progn
  (require 'ov)
  (with-temp-buffer
    (insert "hello world")
    (let ((o (ov 1 6)))
      (goto-char 6)
      (insert "XXXXX")
      (list (ov-length o) (overlay-start o) (overlay-end o) (ov-string o)))))"#,
        &["ov"],
        &ov_dir(),
        expect,
    );
}
