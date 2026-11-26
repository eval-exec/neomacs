//! Permanent GNU Emacs 31.1 oracle matrix and divergence ratchet for
//! `decode-coding-string`, `encode-coding-string`, and coding/EOL text-property
//! semantics.
//!
//! # GNU Emacs 31.1 Source Contract (`a360712c9d272d950d8d8255ef74570f7e90b7d9`)
//!
//! All expected values in this module and in `eol_decode_matrix_gnu31.tsv`
//! come strictly from the attested GNU Emacs 31.1 oracle binary (never from
//! Neomacs's current behavior) and reflect `src/coding.c`:
//! - `code_convert_string` (`src/coding.c:9579-9649`):
//!   - `:9588-9594`: when `coding_system` is `nil`, returns
//!     `nocopy ? string : Fcopy_sequence (string)` **only** when `NILP (dst_object)`.
//!   - `:9596-9603`: when `coding_system` is `nil` and `dst_object` is non-nil
//!     (`t` or a buffer), rewrites `coding_system` to `Qno_conversion`.
//!   - `:9610-9630`: the ASCII-only `nocopy` identity fast path is guarded by
//!     `EQ (dst_object, Qt)` (string return); it checks
//!     `CODING_ATTR_ASCII_COMPAT`, `Qunix || inhibit_eol_conversion || !memchr(..., encodep ? '\n' : '\r', bytes)`,
//!     and when `nocopy` is false allocates a fresh property-free string via
//!     `make_unibyte_string` (encode) or `make_multibyte_string` (decode).
//!   - `:9631-9648`: when `BUFFERP (dst_object)`, GNU always runs
//!     `encode_coding_object` / `decode_coding_object` (`:9640, :9642`), never
//!     copies source string text properties into the buffer, preserves point,
//!     and returns `make_fixnum (coding.produced_char)`.
//! - `Fdecode_coding_string` / `Fencode_coding_string` (`src/coding.c:10491-10531`):
//!   - `:10500-10502`: if the destination buffer is unibyte, it receives the
//!     individual bytes of the internal representation of the decoded text.
//!   - `:10506`: neither function alters the match data.
//! - `produce_charset` (`src/coding.c:7253-7261`), `decode_coding`
//!   (`src/coding.c:7405-7406`), and `decode_eol` (`src/coding.c:6761-6849`):
//!   - `produce_charset` attaches `(charset iso-8859-1)` to decoded ranges, and
//!     `decode_eol` removes `\r` in place via `del_range_2` (`:6839`), preserving
//!     those `charset` properties across DOS/Mac EOL conversion.
//!
//! # Environment Independence
//!
//! Every coding system exercised here (`utf-8*`, `raw-text*`, `undecided*`,
//! `iso-latin-1*`, `unix`/`dos`/`mac`, `nil`, `no-conversion`, explicit
//! `define-coding-system-alias` targets, and `utf-16be-with-signature`) is
//! pure deterministic Emacs C logic under `-Q --batch` with zero host-library,
//! locale, or `iconv` dependence.
//!
//! # Reproducing `eol_decode_matrix_gnu31.tsv`
//!
//! The 2,898-line checked-in reference table (`<case-id>\t<gnu-31.1-result>`)
//! is generated from the attested GNU Emacs 31.1 oracle (`scripts/ci/check-gnu-oracle.sh`):
//!
//! ```text
//! $NEOMACS_ORACLE_BIN -Q --batch --eval '<FULL_MATRIX_PROBE_ELISP>' \
//!   > crates/neovm-oracle-tests/src/coding/eol_decode_matrix_gnu31.tsv
//! ```
//!
//! Under `NEOVM_ORACLE_MODE=verify` (or `live`), every row in this module ALSO
//! executes against the live attested GNU Emacs 31.1 oracle and asserts zero
//! drift between live GNU Emacs 31.1 and the checked-in GNU expectations.
//!
//! # Tracked Divergences (Ratchet Ceilings)
//!
//! On `77h` (`e57c8d8b7c`, after 77g `f41b3eacc7` closed Classes B & D and 77h
//! `cc67b23c64` closed Class C), 2,556 / 2,898 cases match GNU Emacs 31.1 and
//! 342 / 2,898 diverge across exactly 2 remaining tracked classes (0 unexplained
//! residuals):
//! - Class A (#846): 324 cases — missing `(charset iso-8859-1)` on `iso-latin-1*` / `undecided*` decode
//! - Class B (#847):   0 cases — fixed in 77g (`f41b3eacc7`, tightened 217 -> 0)
//! - Class C (#848):   0 cases — fixed in 77h (`cc67b23c64`, tightened 98 -> 0)
//! - Class D (#849):   0 cases — fixed in 77g (`f41b3eacc7`, tightened 21 -> 0)
//! - Class E (#854):  18 cases — `vietnamese-viqr` conversion-hook buffer-dst fast-path guard
//!   (`encoding.rs:5695`, 16 ASCII buffer-dst rows) and unibyte-buffer `:post-read-conversion`
//!   multibyteness (`src/coding.c:8155-8197`, 2 `uni-viqr-mixed` `buf-uni` rows)
//!
//! All tracked cases are guarded by:
//! - the always-on `coding_matrix_known_divergence_ratchet` (fails if any
//!   class ceiling or total grows, or if any unclassified divergence appears), and
//! - the opt-in `--features coding-known-divergences` suite (strict GNU-expected
//!   per-issue rows, zero `#[ignore]`).

use std::collections::BTreeMap;

use crate::common;

const GNU_31_TABLE_TSV: &str = include_str!("eol_decode_matrix_gnu31.tsv");

// Tightened orthogonal ratchet ceilings on `77h` (`e57c8d8b7c`):
// Classes B (#847), C (#848), and D (#849) are closed at 0; only Class A (#846,
// 324 cases) and Class E (#854, 18 `vietnamese-viqr` buffer-dst cases) remain
// (324 + 18 = 342 total distinct diverging cases).
const RATCHET_MAX_TOTAL: usize = 342;
const RATCHET_MAX_CLASS_A_846: usize = 324;
const RATCHET_MAX_CLASS_B_847: usize = 0;
const RATCHET_MAX_CLASS_C_848: usize = 0;
const RATCHET_MAX_CLASS_D_849: usize = 0;
const RATCHET_MAX_CLASS_E_854: usize = 18;

const ELISP_PREAMBLE: &str = r#"
  (define-coding-system-alias 'cm-alias-utf8-dos 'utf-8-dos)
  (define-coding-system-alias 'cm-alias-raw-mac 'raw-text-mac)

  (defun cm-make-src (kind)
    (let ((raw
           (pcase kind
             ('uni-ascii-plain (unibyte-string ?a ?b))
             ('mb-ascii-plain (string-to-multibyte "ab"))
             ('uni-ascii-lf (unibyte-string ?a ?\n ?b ?\n))
             ('mb-ascii-lf (string-to-multibyte "a\nb\n"))
             ('uni-ascii-crlf (unibyte-string ?a ?\r ?\n ?b ?\r ?\n))
             ('mb-ascii-crlf (string-to-multibyte "a\r\nb\r\n"))
             ('uni-ascii-cr (unibyte-string ?a ?\r ?b ?\r))
             ('mb-ascii-cr (string-to-multibyte "a\rb\r"))
             ('uni-utf8-crlf (unibyte-string #xc3 #xa9 ?\r ?\n))
             ('mb-utf8-lf (concat (string #xe9) "\n"))
             ('uni-latin1-crlf (unibyte-string #xe9 ?\r ?\n))
             ('uni-latin1-lf (unibyte-string #xe9 ?\n))
             ('uni-viqr-ascii (unibyte-string ?V ?i ?e ?^ ?. ?t))
             ('mb-viqr-ascii (string-to-multibyte "Vie^.t"))
             ('uni-viqr-mixed (unibyte-string ?V ?i ?e ?^ ?. ?t #xc3 #xa9)))))
      (propertize raw 'foo 'bar)))

  (defun cm-props (str-or-buf)
    (if (bufferp str-or-buf)
        (with-current-buffer str-or-buf
          (let ((out nil) (p (point-min)))
            (while (< p (point-max))
              (let* ((next (or (next-property-change p) (point-max)))
                     (pr (text-properties-at p)))
                (when pr (push (list p next pr) out))
                (setq p next)))
            (nreverse out)))
      (let ((out nil) (p 0) (len (length str-or-buf)))
        (while (< p len)
          (let* ((next (or (next-property-change p str-or-buf) len))
                 (pr (text-properties-at p str-or-buf)))
            (when pr (push (list p next pr) out))
            (setq p next)))
        (nreverse out))))

  (defun cm-run-one (op cs src-kind nocopy dst inhibit-eol)
    (let ((inhibit-eol-conversion inhibit-eol)
          (orig (cm-make-src src-kind))
          (last-coding-system-used 'sentinel))
      (condition-case err
          (pcase dst
            ((or 'nil 't)
             (let ((res (if (eq op 'decode)
                            (decode-coding-string orig cs nocopy dst)
                          (encode-coding-string orig cs nocopy dst))))
               (list 'ok
                     :eq (eq res orig)
                     :mb (multibyte-string-p res)
                     :bytes (string-to-list res)
                     :props (cm-props res)
                     :last last-coding-system-used)))
            ((or 'buf-mb 'buf-uni 'buf-mid)
             (with-temp-buffer
               (when (eq dst 'buf-uni) (set-buffer-multibyte nil))
               (when (eq dst 'buf-mid)
                 (insert (propertize "XY" 'keep 'yes))
                 (goto-char 2))
               (let* ((pt-before (point))
                      (res (if (eq op 'decode)
                               (decode-coding-string orig cs nocopy (current-buffer))
                             (encode-coding-string orig cs nocopy (current-buffer))))
                      (pt-after (point)))
                 (list 'ok
                       :ret res
                       :pt-same (= pt-before pt-after)
                       :mb enable-multibyte-characters
                       :bytes (string-to-list (buffer-string))
                       :props (cm-props (current-buffer))
                       :last last-coding-system-used)))))
        (error (list 'err (car err))))))
"#;

const FULL_MATRIX_BODY: &str = r#"
  (let ((codings '(utf-8 utf-8-unix utf-8-dos utf-8-mac
                   raw-text raw-text-unix raw-text-dos raw-text-mac
                   undecided undecided-unix undecided-dos undecided-mac
                   iso-latin-1 iso-latin-1-unix iso-latin-1-dos iso-latin-1-mac
                   unix dos mac
                   nil no-conversion
                   cm-alias-utf8-dos cm-alias-raw-mac
                   utf-16be-with-signature))
        (decode-srcs '(uni-ascii-plain mb-ascii-plain uni-ascii-lf mb-ascii-lf
                       uni-ascii-crlf mb-ascii-crlf uni-ascii-cr mb-ascii-cr
                       uni-utf8-crlf uni-latin1-crlf uni-latin1-lf))
        (encode-srcs '(uni-ascii-plain mb-ascii-plain uni-ascii-lf mb-ascii-lf
                       mb-utf8-lf))
        (dsts '(nil t buf-mb buf-uni buf-mid)))
    (dolist (cs codings)
      (dolist (sk decode-srcs)
        (dolist (nc '(nil t))
          (dolist (dst dsts)
            (when (or (not (eq dst 'buf-mid))
                      (and (memq cs '(utf-8-dos raw-text-dos undecided-dos dos nil))
                           (memq sk '(uni-ascii-lf uni-ascii-crlf))))
              (princ (format "%S\t%S\n"
                             (list 'decode cs sk nc dst nil)
                             (cm-run-one 'decode cs sk nc dst nil))))))))
    (dolist (cs '(utf-8-dos utf-8-mac raw-text-dos undecided-dos iso-latin-1-dos dos mac))
      (dolist (sk '(uni-ascii-lf uni-ascii-crlf uni-ascii-cr))
        (dolist (nc '(nil t))
          (dolist (dst '(nil buf-mb buf-uni))
            (princ (format "%S\t%S\n"
                           (list 'decode cs sk nc dst t)
                           (cm-run-one 'decode cs sk nc dst t)))))))
    (dolist (cs '(utf-8 utf-8-unix utf-8-dos utf-8-mac
                  raw-text raw-text-dos raw-text-mac
                  iso-latin-1 iso-latin-1-dos
                  unix dos mac nil no-conversion cm-alias-utf8-dos))
      (dolist (sk encode-srcs)
        (dolist (nc '(nil t))
          (dolist (dst '(nil t buf-mb buf-uni))
            (princ (format "%S\t%S\n"
                           (list 'encode cs sk nc dst nil)
                           (cm-run-one 'encode cs sk nc dst nil)))))))
    ;; Conversion-hook (`vietnamese-viqr`) fast-path vs buffer-destination matrix:
    ;; covers string dst (fast path skips hook, src/coding.c:9610-9629),
    ;; buffer dst (`buf-mb`/`buf-uni`, hook runs, src/coding.c:8155-8197),
    ;; and `uni-viqr-mixed` separating decoder UTF-8 bytes from hook truncation.
    (dolist (sk '(uni-viqr-ascii mb-viqr-ascii uni-viqr-mixed))
      (dolist (nc '(nil t))
        (dolist (dst '(nil t buf-mb buf-uni))
          (princ (format "%S\t%S\n"
                         (list 'decode 'vietnamese-viqr sk nc dst nil)
                         (cm-run-one 'decode 'vietnamese-viqr sk nc dst nil))))))
    (dolist (sk '(uni-viqr-ascii mb-viqr-ascii))
      (dolist (nc '(nil t))
        (dolist (dst '(nil t buf-mb buf-uni))
          (princ (format "%S\t%S\n"
                         (list 'encode 'vietnamese-viqr sk nc dst nil)
                         (cm-run-one 'encode 'vietnamese-viqr sk nc dst nil)))))))
  t
"#;

fn parse_tsv_map(transcript: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in transcript.lines() {
        if let Some((k, v)) = line.split_once('\t') {
            if k.starts_with('(') {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

fn load_gnu_table() -> BTreeMap<String, String> {
    let table = parse_tsv_map(GNU_31_TABLE_TSV);
    assert_eq!(
        table.len(),
        2898,
        "eol_decode_matrix_gnu31.tsv must contain all 2,898 attested GNU 31.1 cases"
    );
    table
}

fn build_named_cases_form(cases: &[(&str, &str)]) -> String {
    let mut list = String::new();
    for (case_id, _) in cases {
        list.push_str("    ");
        list.push_str(case_id);
        list.push('\n');
    }
    format!(
        "(progn\n{ELISP_PREAMBLE}\n  (dolist (c '(\n{list}  ))\n    (princ (format \"%S\\t%S\\n\" c (apply #'cm-run-one c))))\n  t)"
    )
}

fn assert_named_cases_match_gnu(suite: &str, form: &str, expected: &[(&str, &str)]) {
    if common::oracle_mode_runs_live_gnu() {
        let gnu_raw = common::run_oracle_eval(form).expect("attested GNU Emacs 31.1 eval");
        let gnu_map = parse_tsv_map(&gnu_raw);
        let mut gnu_drift = Vec::new();
        for &(case_id, exp) in expected {
            match gnu_map.get(case_id) {
                Some(actual) if actual == exp => {}
                other => gnu_drift.push(format!(
                    "  {case_id}\n    CHECKED-IN: {exp}\n    LIVE GNU:   {}",
                    other.map_or("<missing>", String::as_str)
                )),
            }
        }
        assert!(
            gnu_drift.is_empty(),
            "{suite}: checked-in expectations drifted from live attested GNU Emacs 31.1 ({} mismatches):\n{}",
            gnu_drift.len(),
            gnu_drift.join("\n")
        );
    }

    let neo_raw = common::run_neomacs_eval(form).expect("Neomacs eval");
    let neo_map = parse_tsv_map(&neo_raw);
    let mut diffs = Vec::new();
    for &(case_id, exp) in expected {
        match neo_map.get(case_id) {
            Some(actual) if actual == exp => {}
            other => diffs.push(format!(
                "  {case_id}\n    GNU 31.1: {exp}\n    NEOMACS:  {}",
                other.map_or("<missing>", String::as_str)
            )),
        }
    }
    assert!(
        diffs.is_empty(),
        "{suite}: {} / {} cases diverged from GNU Emacs 31.1:\n{}",
        diffs.len(),
        expected.len(),
        diffs.join("\n")
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiffClassification {
    class_a_846: bool,
    class_b_847: bool,
    class_c_848: bool,
    class_d_849: bool,
    class_e_854: bool,
}

impl DiffClassification {
    fn is_classified(self) -> bool {
        self.class_a_846
            || self.class_b_847
            || self.class_c_848
            || self.class_d_849
            || self.class_e_854
    }
}

fn extract_plist_field<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!(" {key} ");
    let start = plist.find(&needle)? + needle.len();
    let rest = &plist[start..];
    let mut end = rest.len();
    for next_key in [
        " :eq ",
        " :ret ",
        " :pt-same ",
        " :mb ",
        " :bytes ",
        " :props ",
        " :last ",
    ] {
        if let Some(pos) = rest.find(next_key) {
            end = end.min(pos);
        }
    }
    if end == rest.len() {
        Some(rest.strip_suffix(')').unwrap_or(rest))
    } else {
        Some(&rest[..end])
    }
}

/// Classify a live `(case_id, gnu_result, neo_result)` mismatch into the five
/// tracked GitHub issues (`#846`, `#847`, `#848`, `#849`, `#854`). Any mismatch
/// that does not fit one of these exact root-cause signatures returns all-false
/// and fails the ratchet immediately as an unclassified regression.
fn classify_divergence(case_id: &str, gnu: &str, neo: &str) -> DiffClassification {
    let parts: Vec<&str> = case_id
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .collect();
    assert_eq!(parts.len(), 6, "malformed case id: {case_id}");
    let (op, cs, _sk, _nc, dst, _inh) =
        (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]);

    let fields = [
        ":eq", ":ret", ":pt-same", ":mb", ":bytes", ":props", ":last",
    ];
    let mut diff_fields = Vec::new();
    for field in fields {
        if extract_plist_field(gnu, field) != extract_plist_field(neo, field) {
            diff_fields.push(field);
        }
    }
    let g_props = extract_plist_field(gnu, ":props").unwrap_or("");
    let n_props = extract_plist_field(neo, ":props").unwrap_or("");

    let mut out = DiffClassification {
        class_a_846: false,
        class_b_847: false,
        class_c_848: false,
        class_d_849: false,
        class_e_854: false,
    };

    // Fail closed if a mid-buffer destination ever drops its pre-existing `(keep yes)` property.
    if g_props.contains("(keep yes)") && !n_props.contains("(keep yes)") {
        return out;
    }

    // Class E (#854): conversion-hook (`vietnamese-viqr`) buffer-destination
    // fast-path omission (`encoding.rs:5695`) and unibyte-buffer `:post-read-conversion`
    // multibyteness (`src/coding.c:8155-8197`). String destinations (`nil`/`t`)
    // and any metadata/property divergence deliberately remain unclassified.
    if cs == "vietnamese-viqr"
        && (dst == "buf-mb" || dst == "buf-uni")
        && !diff_fields.contains(&":pt-same")
        && !diff_fields.contains(&":mb")
        && !diff_fields.contains(&":props")
        && !diff_fields.contains(&":last")
    {
        out.class_e_854 = true;
        return out;
    }

    let missing_iso = op == "decode"
        && g_props.contains("(charset iso-8859-1)")
        && !n_props.contains("(charset iso-8859-1)");
    let leaked_foo = !g_props.contains("(foo bar)") && n_props.contains("(foo bar)");
    let stripped_foo_nil_cs = cs == "nil"
        && dst == "nil"
        && g_props.contains("(foo bar)")
        && !n_props.contains("(foo bar)");

    if diff_fields == [":props"] {
        if missing_iso {
            out.class_a_846 = true;
        }
        if leaked_foo {
            out.class_b_847 = true;
        }
        if stripped_foo_nil_cs {
            out.class_d_849 = true;
        }
        return out;
    }

    if op == "decode"
        && dst == "buf-uni"
        && (diff_fields == [":bytes"]
            || (diff_fields == [":bytes", ":props"] && missing_iso && !leaked_foo))
    {
        out.class_c_848 = true;
        if missing_iso {
            out.class_a_846 = true;
        }
        return out;
    }

    if cs == "nil" && !diff_fields.contains(&":pt-same") && !diff_fields.contains(&":last") {
        out.class_d_849 = true;
        return out;
    }

    out
}

/// Run the full 2,858-case probe on Neomacs (and verify `eol_decode_matrix_gnu31.tsv`
/// against live attested GNU Emacs 31.1 under `NEOVM_ORACLE_MODE=verify|live`).
fn run_full_matrix() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let form = format!("(progn\n{ELISP_PREAMBLE}\n{FULL_MATRIX_BODY})");
    let gnu_table = load_gnu_table();

    if common::oracle_mode_runs_live_gnu() {
        let gnu_raw = common::run_oracle_eval(&form).expect("attested GNU Emacs 31.1 full matrix");
        let gnu_live = parse_tsv_map(&gnu_raw);
        assert_eq!(
            gnu_live.len(),
            gnu_table.len(),
            "live GNU Emacs 31.1 emitted {} cases, expected {}",
            gnu_live.len(),
            gnu_table.len()
        );
        let mut drift = Vec::new();
        for (k, exp) in &gnu_table {
            match gnu_live.get(k) {
                Some(actual) if actual == exp => {}
                other => drift.push(format!(
                    "  {k}\n    TSV:      {exp}\n    LIVE GNU: {}",
                    other.map_or("<missing>", String::as_str)
                )),
            }
        }
        assert!(
            drift.is_empty(),
            "eol_decode_matrix_gnu31.tsv drifted from live attested GNU Emacs 31.1 ({} diffs):\n{}",
            drift.len(),
            drift
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    let neo_raw = common::run_neomacs_eval(&form).expect("Neomacs full matrix");
    let neo_live = parse_tsv_map(&neo_raw);
    assert_eq!(
        neo_live.len(),
        gnu_table.len(),
        "Neomacs emitted {} cases, expected {}",
        neo_live.len(),
        gnu_table.len()
    );
    (gnu_table, neo_live)
}

// ===========================================================================
// Default Fast Suite (always enabled, zero #[ignore], ~2s total)
// ===========================================================================

/// Row 1: `decode-coding-string` with DOS/Mac/alias EOL codings and a buffer
/// destination (`NOCOPY = t` and `nil`, multibyte/unibyte/mid-buffer).
///
/// Covers all 13 cases regressed by splitperf `a152723324` (where cloning
/// `orig` into the buffer leaked `(foo bar)` from the source string) plus
/// alias, `NOCOPY = nil`, CRLF/CR rewrite, and mid-buffer point preservation.
/// GNU `src/coding.c:9610, 9631-9648`: `nocopy` is ignored for buffer
/// destinations and source string text properties never leak into the buffer.
#[test]
fn coding_matrix_decode_buffer_dos_mac_no_property_leak() {
    const EXPECTED: &[(&str, &str)] = &[
        // The 13 cases regressed by splitperf a152723324 (must leave :props nil):
        (
            "(decode utf-8-dos uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-mac uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-mac)",
        ),
        (
            "(decode raw-text-dos uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last raw-text-dos)",
        ),
        (
            "(decode raw-text-mac uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last raw-text-mac)",
        ),
        (
            "(decode undecided-dos uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last undecided-dos)",
        ),
        (
            "(decode undecided-mac uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last undecided-mac)",
        ),
        (
            "(decode dos uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last dos)",
        ),
        (
            "(decode mac uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last mac)",
        ),
        (
            "(decode utf-8-dos mb-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-mac mb-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-mac)",
        ),
        (
            "(decode raw-text-dos mb-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last raw-text-dos)",
        ),
        (
            "(decode utf-8-dos uni-ascii-lf t buf-uni nil)",
            "(ok :ret 4 :pt-same t :mb nil :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode raw-text-dos uni-ascii-lf t buf-uni nil)",
            "(ok :ret 4 :pt-same t :mb nil :bytes (97 10 98 10) :props nil :last raw-text-dos)",
        ),
        // Custom aliases to DOS/Mac codings:
        (
            "(decode cm-alias-utf8-dos uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last cm-alias-utf8-dos)",
        ),
        (
            "(decode cm-alias-raw-mac uni-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last cm-alias-raw-mac)",
        ),
        // NOCOPY=nil twin, actual CRLF/CR conversion, and mid-buffer point/property preservation:
        (
            "(decode utf-8-dos uni-ascii-lf nil buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-dos uni-ascii-crlf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-mac uni-ascii-cr t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-mac)",
        ),
        (
            "(decode raw-text-dos uni-ascii-crlf t buf-uni nil)",
            "(ok :ret 4 :pt-same t :mb nil :bytes (97 10 98 10) :props nil :last raw-text-dos)",
        ),
        (
            "(decode dos uni-ascii-crlf t buf-mid nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (88 97 10 98 10 89) :props ((1 2 (keep yes)) (6 7 (keep yes))) :last dos)",
        ),
        (
            "(decode utf-8-dos uni-ascii-lf t buf-mid nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (88 97 10 98 10 89) :props ((1 2 (keep yes)) (6 7 (keep yes))) :last utf-8-dos)",
        ),
        (
            "(decode raw-text-dos uni-ascii-lf t buf-mid nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (88 97 10 98 10 89) :props ((1 2 (keep yes)) (6 7 (keep yes))) :last raw-text-dos)",
        ),
        (
            "(decode undecided-dos uni-ascii-lf t buf-mid nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (88 97 10 98 10 89) :props ((1 2 (keep yes)) (6 7 (keep yes))) :last undecided-dos)",
        ),
        (
            "(decode utf-8-dos uni-utf8-crlf t buf-mb nil)",
            "(ok :ret 2 :pt-same t :mb t :bytes (233 10) :props nil :last utf-8-dos)",
        ),
    ];
    let form = build_named_cases_form(EXPECTED);
    assert_named_cases_match_gnu(
        "coding_matrix_decode_buffer_dos_mac_no_property_leak",
        &form,
        EXPECTED,
    );
}

/// Row 2: `decode-coding-string` with string destinations (`BUFFER` = `nil` and `t`),
/// checking `eq` identity, property retention vs stripping, `multibyte-string-p`,
/// `last-coding-system-used`, aliases, and non-ASCII-compatible `utf-16be-with-signature`
/// (GNU `src/coding.c:9592, 9610-9629`).
#[test]
fn coding_matrix_decode_string_dst_and_nocopy_identity() {
    const EXPECTED: &[(&str, &str)] = &[
        // ASCII-compatible fast path (src/coding.c:9615-9629): nocopy=nil strips props
        // and returns multibyte; nocopy=t returns `eq` orig with props intact.
        (
            "(decode utf-8 uni-ascii-plain nil nil nil)",
            "(ok :eq nil :mb t :bytes (97 98) :props nil :last utf-8)",
        ),
        (
            "(decode utf-8 uni-ascii-plain t nil nil)",
            "(ok :eq t :mb nil :bytes (97 98) :props ((0 2 (foo bar))) :last utf-8)",
        ),
        (
            "(decode utf-8 uni-ascii-plain nil t nil)",
            "(ok :eq nil :mb t :bytes (97 98) :props nil :last utf-8)",
        ),
        (
            "(decode utf-8 uni-ascii-plain t t nil)",
            "(ok :eq t :mb nil :bytes (97 98) :props ((0 2 (foo bar))) :last utf-8)",
        ),
        // No `\r` in input -> decode fast path triggers even for -dos/-mac (src/coding.c:9620):
        (
            "(decode utf-8-dos uni-ascii-lf t nil nil)",
            "(ok :eq t :mb nil :bytes (97 10 98 10) :props ((0 4 (foo bar))) :last utf-8-dos)",
        ),
        // `\r` present -> misses fast path, strips props and rewrites EOL:
        (
            "(decode utf-8-dos uni-ascii-crlf t nil nil)",
            "(ok :eq nil :mb t :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-mac uni-ascii-cr t nil nil)",
            "(ok :eq nil :mb t :bytes (97 10 98 10) :props nil :last utf-8-mac)",
        ),
        (
            "(decode raw-text-dos uni-ascii-lf t nil nil)",
            "(ok :eq t :mb nil :bytes (97 10 98 10) :props ((0 4 (foo bar))) :last raw-text-dos)",
        ),
        (
            "(decode raw-text-dos uni-ascii-crlf t nil nil)",
            "(ok :eq nil :mb nil :bytes (97 10 98 10) :props nil :last raw-text-dos)",
        ),
        (
            "(decode undecided-dos uni-ascii-crlf t nil nil)",
            "(ok :eq nil :mb t :bytes (97 10 98 10) :props nil :last undecided-dos)",
        ),
        (
            "(decode dos uni-ascii-crlf t nil nil)",
            "(ok :eq nil :mb t :bytes (97 10 98 10) :props nil :last dos)",
        ),
        (
            "(decode mac uni-ascii-cr t nil nil)",
            "(ok :eq nil :mb t :bytes (97 10 98 10) :props nil :last mac)",
        ),
        (
            "(decode unix uni-ascii-lf t nil nil)",
            "(ok :eq t :mb nil :bytes (97 10 98 10) :props ((0 4 (foo bar))) :last unix)",
        ),
        (
            "(decode no-conversion uni-ascii-plain nil nil nil)",
            "(ok :eq nil :mb t :bytes (97 98) :props nil :last no-conversion)",
        ),
        (
            "(decode no-conversion uni-ascii-plain t nil nil)",
            "(ok :eq t :mb nil :bytes (97 98) :props ((0 2 (foo bar))) :last no-conversion)",
        ),
        (
            "(decode nil uni-ascii-plain t nil nil)",
            "(ok :eq t :mb nil :bytes (97 98) :props ((0 2 (foo bar))) :last no-conversion)",
        ),
        (
            "(decode nil uni-ascii-plain t t nil)",
            "(ok :eq t :mb nil :bytes (97 98) :props ((0 2 (foo bar))) :last no-conversion)",
        ),
        (
            "(decode cm-alias-utf8-dos uni-ascii-crlf t nil nil)",
            "(ok :eq nil :mb t :bytes (97 10 98 10) :props nil :last cm-alias-utf8-dos)",
        ),
        (
            "(decode cm-alias-raw-mac uni-ascii-cr t nil nil)",
            "(ok :eq nil :mb nil :bytes (97 10 98 10) :props nil :last cm-alias-raw-mac)",
        ),
        // Non-ASCII-compatible coding (CODING_ATTR_ASCII_COMPAT nil, src/coding.c:9615):
        (
            "(decode utf-16be-with-signature uni-ascii-plain t nil nil)",
            "(ok :eq nil :mb t :bytes (24930) :props nil :last utf-16be-with-signature)",
        ),
        (
            "(decode utf-8-dos uni-utf8-crlf t nil nil)",
            "(ok :eq nil :mb t :bytes (233 10) :props nil :last utf-8-dos)",
        ),
    ];
    let form = build_named_cases_form(EXPECTED);
    assert_named_cases_match_gnu(
        "coding_matrix_decode_string_dst_and_nocopy_identity",
        &form,
        EXPECTED,
    );
}

/// Row 3: Symmetric `encode-coding-string` across string and buffer destinations,
/// `NOCOPY = nil/t`, DOS/Mac/Unix EOL conversion, `raw-text`, `iso-latin-1`,
/// `no-conversion`, `nil`, and custom aliases (`src/coding.c:9580-9649, 10513-10531`).
#[test]
fn coding_matrix_encode_symmetric_matrix() {
    const EXPECTED: &[(&str, &str)] = &[
        (
            "(encode utf-8 mb-ascii-plain nil nil nil)",
            "(ok :eq nil :mb nil :bytes (97 98) :props nil :last utf-8)",
        ),
        (
            "(encode utf-8 mb-ascii-plain t nil nil)",
            "(ok :eq t :mb t :bytes (97 98) :props ((0 2 (foo bar))) :last utf-8)",
        ),
        (
            "(encode utf-8-unix mb-ascii-lf t nil nil)",
            "(ok :eq t :mb t :bytes (97 10 98 10) :props ((0 4 (foo bar))) :last utf-8-unix)",
        ),
        // `\n` present -> misses encode -dos/-mac fast path (src/coding.c:9620):
        (
            "(encode utf-8-dos mb-ascii-lf nil nil nil)",
            "(ok :eq nil :mb nil :bytes (97 13 10 98 13 10) :props nil :last utf-8-dos)",
        ),
        (
            "(encode utf-8-dos mb-ascii-lf t nil nil)",
            "(ok :eq nil :mb nil :bytes (97 13 10 98 13 10) :props nil :last utf-8-dos)",
        ),
        (
            "(encode utf-8-dos mb-ascii-lf t buf-mb nil)",
            "(ok :ret 6 :pt-same t :mb t :bytes (97 13 10 98 13 10) :props nil :last utf-8-dos)",
        ),
        (
            "(encode utf-8-dos mb-ascii-lf t buf-uni nil)",
            "(ok :ret 6 :pt-same t :mb nil :bytes (97 13 10 98 13 10) :props nil :last utf-8-dos)",
        ),
        (
            "(encode utf-8-mac mb-ascii-lf t nil nil)",
            "(ok :eq nil :mb nil :bytes (97 13 98 13) :props nil :last utf-8-mac)",
        ),
        (
            "(encode utf-8-mac mb-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 13 98 13) :props nil :last utf-8-mac)",
        ),
        (
            "(encode raw-text-dos mb-ascii-lf t buf-mb nil)",
            "(ok :ret 6 :pt-same t :mb t :bytes (97 13 10 98 13 10) :props nil :last raw-text-dos)",
        ),
        (
            "(encode raw-text-mac mb-ascii-lf t buf-uni nil)",
            "(ok :ret 4 :pt-same t :mb nil :bytes (97 13 98 13) :props nil :last raw-text-mac)",
        ),
        (
            "(encode iso-latin-1 mb-utf8-lf nil nil nil)",
            "(ok :eq nil :mb nil :bytes (233 10) :props nil :last iso-latin-1)",
        ),
        (
            "(encode iso-latin-1-dos mb-utf8-lf t buf-mb nil)",
            "(ok :ret 3 :pt-same t :mb t :bytes (4194281 13 10) :props nil :last iso-latin-1-dos)",
        ),
        (
            "(encode dos mb-ascii-lf t buf-mb nil)",
            "(ok :ret 6 :pt-same t :mb t :bytes (97 13 10 98 13 10) :props nil :last dos)",
        ),
        (
            "(encode mac mb-ascii-lf t buf-mb nil)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 13 98 13) :props nil :last mac)",
        ),
        (
            "(encode no-conversion mb-utf8-lf t nil nil)",
            "(ok :eq nil :mb nil :bytes (195 169 10) :props nil :last no-conversion)",
        ),
        (
            "(encode nil mb-ascii-plain t nil nil)",
            "(ok :eq t :mb t :bytes (97 98) :props ((0 2 (foo bar))) :last no-conversion)",
        ),
        (
            "(encode cm-alias-utf8-dos mb-ascii-lf t buf-mb nil)",
            "(ok :ret 6 :pt-same t :mb t :bytes (97 13 10 98 13 10) :props nil :last cm-alias-utf8-dos)",
        ),
    ];
    let form = build_named_cases_form(EXPECTED);
    assert_named_cases_match_gnu("coding_matrix_encode_symmetric_matrix", &form, EXPECTED);
}

/// Row 4: `inhibit-eol-conversion = t` (`src/coding.c:9619, 6767`), match-data
/// preservation (`src/coding.c:10506`), and symmetric `decode-coding-region` /
/// `encode-coding-region` EOL + property behavior.
#[test]
fn coding_matrix_inhibit_eol_and_match_data_and_region() {
    const EXPECTED: &[(&str, &str)] = &[
        (
            "(decode utf-8-dos uni-ascii-lf nil buf-mb t)",
            "(ok :ret 4 :pt-same t :mb t :bytes (97 10 98 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-dos uni-ascii-crlf nil buf-mb t)",
            "(ok :ret 6 :pt-same t :mb t :bytes (97 13 10 98 13 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-dos uni-ascii-crlf t buf-mb t)",
            "(ok :ret 6 :pt-same t :mb t :bytes (97 13 10 98 13 10) :props nil :last utf-8-dos)",
        ),
        (
            "(decode utf-8-mac uni-ascii-cr t buf-uni t)",
            "(ok :ret 4 :pt-same t :mb nil :bytes (97 13 98 13) :props nil :last utf-8-mac)",
        ),
        (
            "(decode raw-text-dos uni-ascii-crlf t nil t)",
            "(ok :eq t :mb nil :bytes (97 13 10 98 13 10) :props ((0 6 (foo bar))) :last raw-text-dos)",
        ),
        (
            "(decode dos uni-ascii-crlf t nil t)",
            "(ok :eq t :mb nil :bytes (97 13 10 98 13 10) :props ((0 6 (foo bar))) :last dos)",
        ),
        (
            "(decode mac uni-ascii-cr t nil t)",
            "(ok :eq t :mb nil :bytes (97 13 98 13) :props ((0 4 (foo bar))) :last mac)",
        ),
        (
            "(match-after-decode)",
            "((0 11 0 5 6 11) \"hello\" \"world\")",
        ),
        (
            "(match-after-encode)",
            "((0 11 0 5 6 11) \"hello\" \"world\")",
        ),
        ("(decode-region utf-8-unix)", "((97 13 10 98 13 10) nil)"),
        ("(decode-region utf-8-dos)", "((97 10 98 10) nil)"),
        ("(decode-region utf-8-mac)", "((97 10 10 98 10 10) nil)"),
        ("(decode-region raw-text-dos)", "((97 10 98 10) nil)"),
        ("(encode-region utf-8-unix)", "((97 10 98 10) nil)"),
        ("(encode-region utf-8-dos)", "((97 13 10 98 13 10) nil)"),
        ("(encode-region utf-8-mac)", "((97 13 98 13) nil)"),
        ("(encode-region raw-text-dos)", "((97 13 10 98 13 10) nil)"),
    ];

    let form = format!(
        r#"(progn
{ELISP_PREAMBLE}
  (dolist (c '(
      (decode utf-8-dos uni-ascii-lf nil buf-mb t)
      (decode utf-8-dos uni-ascii-crlf nil buf-mb t)
      (decode utf-8-dos uni-ascii-crlf t buf-mb t)
      (decode utf-8-mac uni-ascii-cr t buf-uni t)
      (decode raw-text-dos uni-ascii-crlf t nil t)
      (decode dos uni-ascii-crlf t nil t)
      (decode mac uni-ascii-cr t nil t)))
    (princ (format "%S\t%S\n" c (apply #'cm-run-one c))))
  (let ((s "hello world"))
    (string-match "\\(hello\\) \\(world\\)" s)
    (decode-coding-string (propertize (unibyte-string ?a ?\r ?\n) 'foo 'bar) 'utf-8-dos)
    (princ (format "(match-after-decode)\t%S\n" (list (match-data) (match-string 1 s) (match-string 2 s))))
    (encode-coding-string (propertize "a\nb\n" 'foo 'bar) 'utf-8-dos)
    (princ (format "(match-after-encode)\t%S\n" (list (match-data) (match-string 1 s) (match-string 2 s)))))
  (dolist (cs '(utf-8-unix utf-8-dos utf-8-mac raw-text-dos))
    (with-temp-buffer
      (insert (propertize (unibyte-string ?a ?\r ?\n ?b ?\r ?\n) 'foo 'bar))
      (decode-coding-region (point-min) (point-max) cs)
      (princ (format "(decode-region %S)\t%S\n" cs (list (string-to-list (buffer-string)) (text-properties-at (point-min)))))))
  (dolist (cs '(utf-8-unix utf-8-dos utf-8-mac raw-text-dos))
    (with-temp-buffer
      (insert (propertize "a\nb\n" 'foo 'bar))
      (encode-coding-region (point-min) (point-max) cs)
      (princ (format "(encode-region %S)\t%S\n" cs (list (string-to-list (buffer-string)) (text-properties-at (point-min)))))))
  t)"#
    );
    assert_named_cases_match_gnu(
        "coding_matrix_inhibit_eol_and_match_data_and_region",
        &form,
        EXPECTED,
    );
}

/// Row 5: `vietnamese-viqr` string-destination ASCII fast path and mixed
/// non-ASCII conversion-hook parity (`src/coding.c:9610-9630, 8155-8197`).
///
/// Pins GNU's contract that string destinations (`nil`/`t`) on pure-ASCII input
/// take the fast path and skip `:post-read-conversion` / `:pre-write-conversion`
/// (guarding against an over-corrected fast-path check), while non-ASCII input
/// (`uni-viqr-mixed`) runs the hook on string and multibyte-buffer destinations.
#[test]
fn coding_matrix_viqr_string_dst_fast_path_skips_hook() {
    const EXPECTED: &[(&str, &str)] = &[
        // String destinations on pure-ASCII VIQR escape MUST stay unchanged (fast path skips hook):
        (
            "(decode vietnamese-viqr uni-viqr-ascii nil nil nil)",
            "(ok :eq nil :mb t :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-ascii nil t nil)",
            "(ok :eq nil :mb t :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-ascii t nil nil)",
            "(ok :eq t :mb nil :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-ascii t t nil)",
            "(ok :eq t :mb nil :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr mb-viqr-ascii nil nil nil)",
            "(ok :eq nil :mb t :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr mb-viqr-ascii nil t nil)",
            "(ok :eq nil :mb t :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr mb-viqr-ascii t nil nil)",
            "(ok :eq t :mb t :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr mb-viqr-ascii t t nil)",
            "(ok :eq t :mb t :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr uni-viqr-ascii nil nil nil)",
            "(ok :eq nil :mb nil :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr uni-viqr-ascii nil t nil)",
            "(ok :eq nil :mb nil :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr uni-viqr-ascii t nil nil)",
            "(ok :eq t :mb nil :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr uni-viqr-ascii t t nil)",
            "(ok :eq t :mb nil :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr mb-viqr-ascii nil nil nil)",
            "(ok :eq nil :mb nil :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr mb-viqr-ascii nil t nil)",
            "(ok :eq nil :mb nil :bytes (86 105 101 94 46 116) :props nil :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr mb-viqr-ascii t nil nil)",
            "(ok :eq t :mb t :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        (
            "(encode vietnamese-viqr mb-viqr-ascii t t nil)",
            "(ok :eq t :mb t :bytes (86 105 101 94 46 116) :props ((0 6 (foo bar))) :last vietnamese-viqr)",
        ),
        // Non-ASCII mixed source (`"Vie^.t"` + UTF-8 `é`) bypasses fast path and runs hook:
        (
            "(decode vietnamese-viqr uni-viqr-mixed nil nil nil)",
            "(ok :eq nil :mb t :bytes (86 105 7879 116 233) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-mixed nil t nil)",
            "(ok :eq nil :mb t :bytes (86 105 7879 116 233) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-mixed t nil nil)",
            "(ok :eq nil :mb t :bytes (86 105 7879 116 233) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-mixed t t nil)",
            "(ok :eq nil :mb t :bytes (86 105 7879 116 233) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-mixed nil buf-mb nil)",
            "(ok :ret 5 :pt-same t :mb t :bytes (86 105 7879 116 233) :props nil :last vietnamese-viqr)",
        ),
        (
            "(decode vietnamese-viqr uni-viqr-mixed t buf-mb nil)",
            "(ok :ret 5 :pt-same t :mb t :bytes (86 105 7879 116 233) :props nil :last vietnamese-viqr)",
        ),
    ];
    let form = build_named_cases_form(EXPECTED);
    assert_named_cases_match_gnu(
        "coding_matrix_viqr_string_dst_fast_path_skips_hook",
        &form,
        EXPECTED,
    );
}

/// Row 6 (RATCHET): Runs ALL 2,898 cases of the full coding/EOL matrix on
/// Neomacs (and verifies `eol_decode_matrix_gnu31.tsv` against live attested
/// GNU Emacs 31.1 under `NEOVM_ORACLE_MODE=verify|live`), asserting that:
/// - zero unclassified divergences exist,
/// - no tracked divergence class exceeds its ratchet ceiling
///   (Class A #846 <= 324, Class B #847 <= 0, Class C #848 <= 0, Class D #849 <= 0,
///    Class E #854 <= 18),
/// - total divergences <= 342.
#[test]
fn coding_matrix_known_divergence_ratchet() {
    let (gnu_table, neo_live) = run_full_matrix();

    let mut total = 0usize;
    let mut class_a = Vec::new();
    let mut class_b = Vec::new();
    let mut class_c = Vec::new();
    let mut class_d = Vec::new();
    let mut class_e = Vec::new();
    let mut unexpected = Vec::new();

    for (case_id, gnu_val) in &gnu_table {
        let neo_val = &neo_live[case_id];
        if gnu_val == neo_val {
            continue;
        }
        total += 1;
        let cls = classify_divergence(case_id, gnu_val, neo_val);
        let detail = format!("  {case_id}\n    GNU 31.1: {gnu_val}\n    NEOMACS:  {neo_val}");
        if cls.class_a_846 {
            class_a.push(detail.clone());
        }
        if cls.class_b_847 {
            class_b.push(detail.clone());
        }
        if cls.class_c_848 {
            class_c.push(detail.clone());
        }
        if cls.class_d_849 {
            class_d.push(detail.clone());
        }
        if cls.class_e_854 {
            class_e.push(detail.clone());
        }
        if !cls.is_classified() {
            unexpected.push(detail);
        }
    }

    assert!(
        unexpected.is_empty(),
        "coding_matrix_known_divergence_ratchet: {} unclassified new divergence(s) against GNU Emacs 31.1:\n{}",
        unexpected.len(),
        unexpected
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        class_a.len() <= RATCHET_MAX_CLASS_A_846,
        "Class A (#846, missing charset iso-8859-1) grew: {} > {}\n{}",
        class_a.len(),
        RATCHET_MAX_CLASS_A_846,
        class_a
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        class_b.len() <= RATCHET_MAX_CLASS_B_847,
        "Class B (#847, source property leak into dst) grew: {} > {}\n{}",
        class_b.len(),
        RATCHET_MAX_CLASS_B_847,
        class_b
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        class_c.len() <= RATCHET_MAX_CLASS_C_848,
        "Class C (#848, unibyte buffer truncation) grew: {} > {}\n{}",
        class_c.len(),
        RATCHET_MAX_CLASS_C_848,
        class_c
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        class_d.len() <= RATCHET_MAX_CLASS_D_849,
        "Class D (#849, coding-system=nil dst semantics) grew: {} > {}\n{}",
        class_d.len(),
        RATCHET_MAX_CLASS_D_849,
        class_d
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        class_e.len() <= RATCHET_MAX_CLASS_E_854,
        "Class E (#854, conversion-hook buffer-dst fast-path / unibyte hook multibyteness) grew: {} > {}\n{}",
        class_e.len(),
        RATCHET_MAX_CLASS_E_854,
        class_e
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        total <= RATCHET_MAX_TOTAL,
        "Total coding/EOL divergences grew: {} > {} (A={}, B={}, C={}, D={}, E={})",
        total,
        RATCHET_MAX_TOTAL,
        class_a.len(),
        class_b.len(),
        class_c.len(),
        class_d.len(),
        class_e.len()
    );
}

// ===========================================================================
// Exhaustive Suite (`--features exhaustive-oracle`, zero #[ignore])
// ===========================================================================

#[cfg(feature = "exhaustive-oracle")]
mod exhaustive {
    use super::*;

    fn assert_exhaustive_slice(
        label: &str,
        min_matching: usize,
        predicate: impl Fn(&str, &str, &str, &str, &str, &str) -> bool,
    ) {
        let (gnu_table, neo_live) = run_full_matrix();
        let mut checked = 0usize;
        let mut classified_diffs = Vec::new();
        let mut unexpected_diffs = Vec::new();
        for (case_id, gnu_val) in &gnu_table {
            let parts: Vec<&str> = case_id
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split_whitespace()
                .collect();
            let (op, cs, sk, nc, dst, inh) =
                (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]);
            if !predicate(op, cs, sk, nc, dst, inh) {
                continue;
            }
            let neo_val = &neo_live[case_id];
            if gnu_val != neo_val {
                let detail =
                    format!("  {case_id}\n    GNU 31.1: {gnu_val}\n    NEOMACS:  {neo_val}");
                let cls = classify_divergence(case_id, gnu_val, neo_val);
                if cls.is_classified() {
                    classified_diffs.push(detail);
                } else {
                    unexpected_diffs.push(detail);
                }
            } else {
                checked += 1;
            }
        }
        assert!(
            unexpected_diffs.is_empty(),
            "{label}: {} unexpected regressions:\n{}",
            unexpected_diffs.len(),
            unexpected_diffs
                .iter()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            checked >= min_matching,
            "{label}: GNU-matching count regressed ({checked} < {min_matching}; {} classified diffs):\n{}",
            classified_diffs.len(),
            classified_diffs
                .iter()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn coding_matrix_exhaustive_decode_string_dst() {
        assert_exhaustive_slice(
            "coding_matrix_exhaustive_decode_string_dst",
            938,
            |op, _cs, _sk, _nc, dst, inh| {
                op == "decode" && inh == "nil" && (dst == "nil" || dst == "t")
            },
        );
    }

    #[test]
    fn coding_matrix_exhaustive_decode_buffer_dst() {
        assert_exhaustive_slice(
            "coding_matrix_exhaustive_decode_buffer_dst",
            706,
            |op, _cs, _sk, _nc, dst, inh| {
                op == "decode"
                    && inh == "nil"
                    && (dst == "buf-mb" || dst == "buf-uni" || dst == "buf-mid")
            },
        );
    }

    #[test]
    fn coding_matrix_exhaustive_decode_inhibit_eol() {
        assert_exhaustive_slice(
            "coding_matrix_exhaustive_decode_inhibit_eol",
            102,
            |op, _cs, _sk, _nc, _dst, inh| op == "decode" && inh == "t",
        );
    }

    #[test]
    fn coding_matrix_exhaustive_encode_all_dst() {
        assert_exhaustive_slice(
            "coding_matrix_exhaustive_encode_all_dst",
            526,
            |op, _cs, _sk, _nc, _dst, _inh| op == "encode",
        );
    }
}

// ===========================================================================
// Known-Divergence Suite (`--features coding-known-divergences`, zero #[ignore])
// Strictly GNU-expected; fails RED on current main until each tracked issue is fixed.
// ===========================================================================

#[cfg(feature = "coding-known-divergences")]
mod known_divergences {
    use super::*;

    fn assert_class_zero_divergences(label: &str, select: impl Fn(DiffClassification) -> bool) {
        let (gnu_table, neo_live) = run_full_matrix();
        let mut diffs = Vec::new();
        for (case_id, gnu_val) in &gnu_table {
            let neo_val = &neo_live[case_id];
            if gnu_val == neo_val {
                continue;
            }
            if select(classify_divergence(case_id, gnu_val, neo_val)) {
                diffs.push(format!(
                    "  {case_id}\n    GNU 31.1: {gnu_val}\n    NEOMACS:  {neo_val}"
                ));
            }
        }
        assert!(
            diffs.is_empty(),
            "{label}: {} cases still diverge from GNU Emacs 31.1:\n{}",
            diffs.len(),
            diffs
                .iter()
                .take(25)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Issue #846: missing `(charset iso-8859-1)` text property on decode (324 cases on 9ca34ffd1c).
    #[test]
    fn coding_matrix_known_div_class_a_missing_charset_iso8859_1_issue_846() {
        assert_class_zero_divergences("Class A (#846)", |c| c.class_a_846);
    }

    /// Issue #847: source string `(foo bar)` property leak into destination buffer / dst=t (217 cases on 9ca34ffd1c).
    #[test]
    fn coding_matrix_known_div_class_b_buffer_property_leak_issue_847() {
        assert_class_zero_divergences("Class B (#847)", |c| c.class_b_847);
    }

    /// Issue #848: decode of non-ASCII into unibyte buffer truncates instead of internal UTF-8 bytes (98 cases on 9ca34ffd1c).
    #[test]
    fn coding_matrix_known_div_class_c_unibyte_buffer_truncation_issue_848() {
        assert_class_zero_divergences("Class C (#848)", |c| c.class_c_848);
    }

    /// Issue #849: `coding-system = nil` mishandles `dst = nil` vs `dst = t` vs buffer (0 cases on 77h).
    #[test]
    fn coding_matrix_known_div_class_d_nil_coding_dst_semantics_issue_849() {
        assert_class_zero_divergences("Class D (#849)", |c| c.class_d_849);
    }

    /// Issue #854: `vietnamese-viqr` conversion-hook buffer-destination fast-path omission
    /// (`encoding.rs:5695`, 16 ASCII buffer-dst rows) and unibyte-buffer `:post-read-conversion`
    /// multibyteness (`src/coding.c:8155-8197`, 2 `uni-viqr-mixed` `buf-uni` rows).
    #[test]
    fn coding_matrix_known_div_class_e_conversion_hook_buffer_dst_issue_854() {
        assert_class_zero_divergences("Class E (#854)", |c| c.class_e_854);
    }
}
