//! GNU 31.1 count/byte identity fixtures. Refresh exclusively with UPDATE_EXPECT=1.
//! These observations never iterate a malformed multibyte symbol name. GNU's
//! declared SCHARS need not agree with byte decoding after a mixed-domain
//! shorthand substitution (src/lread.c:5004; src/alloc.c:2241-2247).
use crate::test_utils::runtime_startup_eval_one;
use std::path::PathBuf;

fn assert_gnu(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let script = root.join("../../tmp").join(format!(
            "shorthand-declared-{name}-{}.el",
            std::process::id()
        ));
        std::fs::write(
            &script,
            format!(
                ";;; -*- lexical-binding: nil; coding: utf-8; -*-\n\
                 (unless (equal emacs-version \"31.1\") (error \"GNU 31.1 required\"))\n\
                 (prin1 {form})"
            ),
        )
        .expect("write GNU fixture refresh program in worktree tmp");
        let emacs = crate::test_utils::gnu_oracle().executable_path();
        let sandbox = std::env::var_os("NEOVM_LISP_SANDBOX").unwrap_or_else(|| {
            "/home/exec/Projects/github.com/eval-exec/neomacs-main/tmp/v10x/sandbox-run.sh".into()
        });
        let output = std::process::Command::new(sandbox)
            .arg(emacs)
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .expect("sandboxed GNU fixture refresh");
        assert!(
            output.status.success() && output.stderr.is_empty(),
            "GNU refresh failed or emitted diagnostics: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = format!(
            "OK {}",
            String::from_utf8(output.stdout).expect("GNU ASCII metadata output")
        );
        let path = root
            .join("src/emacs_core/lisp/reader/tests/shorthand_declared")
            .join(format!("{name}.expect"));
        let saved = format!("{text}\n");
        if std::fs::read_to_string(&path).ok().as_deref() != Some(saved.as_str()) {
            std::fs::write(path, saved).expect("save GNU-refreshed fixture");
        }
        text
    } else {
        assert!(
            !fixture.trim().is_empty(),
            "refresh {name} from GNU 31.1 with UPDATE_EXPECT=1 before comparison"
        );
        fixture.trim_end().to_owned()
    };
    assert_eq!(runtime_startup_eval_one(form), expected, "{name}");
}

#[test]
fn symbols_gnu_shorthand_declared_native_shapes() {
    assert_gnu(
        "native_shapes",
        include_str!("shorthand_declared/native_shapes.el"),
        include_str!("shorthand_declared/native_shapes.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_reader_shapes() {
    assert_gnu(
        "reader_shapes",
        include_str!("shorthand_declared/reader_shapes.el"),
        include_str!("shorthand_declared/reader_shapes.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_creation_orders() {
    assert_gnu(
        "creation_orders",
        include_str!("shorthand_declared/creation_orders.el"),
        include_str!("shorthand_declared/creation_orders.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_mixed_prefixes() {
    assert_gnu(
        "mixed_prefixes",
        include_str!("shorthand_declared/mixed_prefixes.el"),
        include_str!("shorthand_declared/mixed_prefixes.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_zero_declared_nonempty_key() {
    assert_gnu(
        "zero_declared_key",
        include_str!("shorthand_declared/zero_declared_key.el"),
        include_str!("shorthand_declared/zero_declared_key.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_valid_raw_utf8_suffix_counts() {
    assert_gnu(
        "valid_raw_utf8_suffix",
        include_str!("shorthand_declared/valid_raw_utf8_suffix.el"),
        include_str!("shorthand_declared/valid_raw_utf8_suffix.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_first_backed_character_access() {
    assert_gnu(
        "first_backed_character",
        include_str!("shorthand_declared/first_backed_character.el"),
        include_str!("shorthand_declared/first_backed_character.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_string_reader_bounds() {
    assert_gnu(
        "declared_reader_bounds",
        include_str!("shorthand_declared/declared_reader_bounds.el"),
        include_str!("shorthand_declared/declared_reader_bounds.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_mixed_reader_character_arithmetic() {
    assert_gnu(
        "mixed_reader_arithmetic",
        include_str!("shorthand_declared/mixed_reader_arithmetic.el"),
        include_str!("shorthand_declared/mixed_reader_arithmetic.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_reader_suffix_cursor() {
    assert_gnu(
        "declared_reader_suffix_cursor",
        include_str!("shorthand_declared/declared_reader_suffix_cursor.el"),
        include_str!("shorthand_declared/declared_reader_suffix_cursor.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_initialized_terminal() {
    assert_gnu(
        "declared_initialized_terminal",
        include_str!("shorthand_declared/declared_initialized_terminal.el"),
        include_str!("shorthand_declared/declared_initialized_terminal.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_unread_original_cache() {
    assert_gnu(
        "declared_unread_cache",
        include_str!("shorthand_declared/declared_unread_cache.el"),
        include_str!("shorthand_declared/declared_unread_cache.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_regex_positions() {
    assert_gnu(
        "declared_regex_positions",
        include_str!("shorthand_declared/declared_regex_positions.el"),
        include_str!("shorthand_declared/declared_regex_positions.expect"),
    );
}

/// GNU syntax.h:156-197 and syntax.c:328-476: every syntax read shares
/// the searched object's signed cache, including a string without intervals.
/// All physical bytes in these fixtures are initialized ASCII; no GNU padding
/// reads or writes are assumed. Expected results must come from GNU 31.1.
#[test]
fn symbols_gnu_shorthand_declared_syntax_states() {
    assert_gnu(
        "declared_syntax_states",
        include_str!("shorthand_declared/declared_syntax_states.el"),
        include_str!("shorthand_declared/declared_syntax_states.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_match_consumers() {
    assert_gnu(
        "declared_match_consumers",
        include_str!("shorthand_declared/declared_match_consumers.el"),
        include_str!("shorthand_declared/declared_match_consumers.expect"),
    );
}

/// GNU regex-emacs.c:4983-5186 converts the current byte offset once, then
/// adjusts logical signed coordinates for previous/current syntax reads.
/// All source bytes are initialized ASCII; fixtures make no padding assumption.
#[test]
fn symbols_gnu_shorthand_declared_syntax_boundaries() {
    assert_gnu(
        "declared_syntax_boundaries",
        include_str!("shorthand_declared/declared_syntax_boundaries.el"),
        include_str!("shorthand_declared/declared_syntax_boundaries.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_replacement_variants() {
    assert_gnu(
        "declared_replacement_variants",
        include_str!("shorthand_declared/declared_replacement_variants.el"),
        include_str!("shorthand_declared/declared_replacement_variants.expect"),
    );
}

/// Declared counts still apply GNU's live CHARACTER_WIDTH and glyph policy
/// to each consumed code (character.c:350,425-440), after mixed-domain rewriting.
#[test]
fn symbols_gnu_shorthand_declared_live_width_policy() {
    assert_gnu(
        "live_width_policy",
        include_str!("shorthand_declared/live_width_policy.el"),
        include_str!("shorthand_declared/live_width_policy.expect"),
    );
}

#[test]
fn symbols_gnu_shorthand_declared_full_code_boundary_syntax() {
    assert_gnu(
        "declared_full_code_boundaries",
        include_str!("shorthand_declared/declared_full_code_boundaries.el"),
        include_str!("shorthand_declared/declared_full_code_boundaries.expect"),
    );
}
