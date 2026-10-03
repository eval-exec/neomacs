//! Supplemental real-engine controls for predecessor box topology,
//! conditional overlay string evaluation, and a GNU Sync refusal.
//! All Context/heap use remains inside the parent full-render fixture.
use super::*;

#[test]
fn lazy_sync_boxed_predecessor_at_line_start_keeps_complete_topology() {
    let text = source();
    let point = text.split_inclusive('\n').take(8).map(str::len).sum();
    // Two printable boundary glyphs and the intervening newline are boxed.
    // The next insertion must preserve the real predecessor's box terminals,
    // not reapply the plain-line Sync lookbehind shortcut.
    let forms = [
        "(put-text-property (- (point) 2) (+ (point) 2) 'face '(:box (:line-width 1 :color \"red\")))",
        "(insert \"x\")",
    ];
    let off = observed(false, false, EditSyncMode::Sync, true, &text, point, &forms);
    let on = observed(true, false, EditSyncMode::Sync, true, &text, point, &forms);
    assert_eq!(on.0, off.0, "published/chrome observers");
    assert!(
        on.3.sync_admissions > 0 && off.3.sync_admissions > 0,
        "real retained Sync must be exercised: on={:?} off={:?}",
        on.3,
        off.3
    );
    let off_edit = off.2.last().unwrap();
    let on_edit = on.2.last().unwrap();
    assert_eq!(on_edit.edit_windows, off_edit.edit_windows);
    assert_eq!(on_edit.relaid_body_rows, off_edit.relaid_body_rows);
    assert_eq!(on_edit.reused_rows, off_edit.reused_rows);
    assert!(
        on_edit.relaid_body_rows >= 2,
        "boxed predecessor must be regenerated: {on_edit:?}"
    );
}

#[test]
fn lazy_sync_conditional_overlay_string_preserves_real_callback_and_full_output() {
    let text = source();
    let forms = [
        r#"(let ((overlay (make-overlay (- (point) 4) (- (point) 4)))
                 (string "O"))
             (put-text-property 0 1 'display
               '(when (progn
                        (setq prove-first-font-count (1+ prove-first-font-count)) t)
                  . "C") string)
             (overlay-put overlay 'before-string string))"#,
        "(insert \"x\")",
    ];
    // The existing observer slot is deliberately used as a numeric display
    // callback counter here. No fontifier is installed by this fixture. The
    // full renderer/reference geometry runs while the same Context is active.
    let off = observed(
        false,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    let on = observed(
        true,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    assert_eq!(
        on.0, off.0,
        "conditional Lisp/display/positional observations"
    );
    for observation in &on.0 {
        assert!(
            observation
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<i64>()
                .unwrap()
                > 0,
            "actual conditional display must run: {observation}"
        );
    }
    assert_eq!(
        on.2.last().unwrap().full_windows,
        off.2.last().unwrap().full_windows
    );
    // Arbitrary conditional display already refuses body reuse; the new knob
    // must leave that canonical producer decision and callback work intact.
    assert_eq!(on.2.last().unwrap().edit_windows, 0);
    assert_eq!(off.2.last().unwrap().edit_windows, 0);
    assert!(on.2.last().unwrap().full_windows > 0);
    // Planning happens before the conditional pre-pass; a preliminary Sync
    // may be discarded. Count accepted rendering, not a planning shortcut.
}

#[test]
fn failed_sync_allowed_keeps_eager_source_reads_and_complete_frame() {
    let text = source();
    let forms = ["(setq word-wrap t)", "(insert \"x\")"];
    let off = observed(
        false,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    let on = observed(
        true,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    assert_eq!(on.0, off.0);
    assert_eq!(
        on.3, off.3,
        "GNU word-wrap refusal must retain eager proof orchestration"
    );
    assert_eq!(on.3.lazy_entries, 0);
    assert_eq!(on.3.sync_admissions, 0);
    assert!(on.3.source_proof_calls > 0 && on.3.char_queries > 0);
    assert_eq!(
        on.2.last().unwrap().relaid_body_rows,
        off.2.last().unwrap().relaid_body_rows
    );
}
