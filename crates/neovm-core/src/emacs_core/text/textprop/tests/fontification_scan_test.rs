//! Raw first-nil admission against independent per-character effective reads.
//! No process policy/environment is changed and no hook runs inside the scan.
use super::*;
use crate::emacs_core::eval::Context;

fn scan(ctx: &Context, from: i64, end: i64) -> FontificationScanAdmission {
    first_unfontified_position_in_state(
        ctx.variables(),
        ctx.buffer_manager(),
        LispCharPos1::new(from),
        LispCharPos1::new(end),
        FontificationBoundaryMode::On,
    )
}

fn assert_raw(ctx: &Context, from: i64, end: i64) {
    // The reference asks the public effective getter for each character; it
    // does not reuse the optimized interval cursor or raw-property predicate.
    let expected = (from..end).find(|pos| {
        builtin_get_char_property_with_frames(
            ctx.variables(),
            ctx.buffer_manager(),
            None,
            &[Value::fixnum(*pos), Value::symbol("fontified")],
        )
        .expect("validated effective reference")
        .is_nil()
    });
    assert_eq!(
        scan(ctx, from, end),
        FontificationScanAdmission::Raw(expected.map(LispCharPos1::new)),
        "{from}..{end}",
    );
    assert_eq!(
        first_unfontified_position_in_state(
            ctx.variables(),
            ctx.buffer_manager(),
            LispCharPos1::new(from),
            LispCharPos1::new(end),
            FontificationBoundaryMode::Off,
        ),
        FontificationScanAdmission::UseGeneric,
    );
}

#[test]
fn fontification_scan_dense_representations_and_all_nonnil_values() {
    for (multibyte, text) in [
        ("nil", "(string-as-unibyte (make-string 12 ?a))"),
        ("nil", "(unibyte-string 65 200 66 255 67 68 69 70)"),
        ("t", "(string-to-multibyte (make-string 12 ?a))"),
        ("t", "\"aé中λbcdé中λ\""),
    ] {
        let mut ctx = Context::new();
        ctx.eval_str(&format!(
            "(progn (set-buffer-multibyte {multibyte}) (insert {text}))"
        ))
        .expect("representation");
        let end = ctx
            .buffer_manager()
            .current_buffer()
            .unwrap()
            .point_max_lisp_char_pos()
            .as_i64();
        for from in 1..end {
            assert_raw(&ctx, from, end);
        }
        ctx.eval_str(
            "(progn
               (put-text-property 1 (point-max) 'fontified t)
               (put-text-property 2 4 'fontified 'defer)
               (put-text-property 4 6 'fontified 0)
               (put-text-property 6 8 'fontified \"truthy\")
               (let ((p 1))
                 (while (< p (point-max))
                   (put-text-property p (1+ p) 'face (if (= (% p 2) 0) 'bold 'italic))
                   (setq p (1+ p)))))",
        )
        .expect("truthy fontified values with unrelated dense splits");
        assert_eq!(scan(&ctx, 1, end), FontificationScanAdmission::Raw(None));
        for from in 1..end {
            for limit in [from + 1, end] {
                assert_raw(&ctx, from, limit);
            }
        }
    }
}

#[test]
fn fontification_scan_holes_implicit_tail_half_open_limits_and_narrowing() {
    let mut ctx = Context::new();
    ctx.eval_str(
        "(progn (insert \"aé中λbcdefg\")
           (put-text-property 2 4 'fontified t)
           (put-text-property 4 6 'fontified nil)
           (put-text-property 6 9 'fontified 'defer))",
    )
    .expect("leading nil, explicit nil island and implicit tail");
    assert_eq!(
        scan(&ctx, 1, 11),
        FontificationScanAdmission::Raw(Some(LispCharPos1::new(1)))
    );
    assert_eq!(scan(&ctx, 2, 4), FontificationScanAdmission::Raw(None));
    assert_eq!(
        scan(&ctx, 2, 11),
        FontificationScanAdmission::Raw(Some(LispCharPos1::new(4)))
    );
    assert_eq!(
        scan(&ctx, 6, 11),
        FontificationScanAdmission::Raw(Some(LispCharPos1::new(9)))
    );
    for from in 1..11 {
        for end in from + 1..=11 {
            assert_raw(&ctx, from, end);
        }
    }
    ctx.eval_str("(narrow-to-region 3 9)").unwrap();
    for from in 3..9 {
        for end in from + 1..=9 {
            assert_raw(&ctx, from, end);
        }
    }
    for (from, end) in [(0, 8), (2, 8), (3, 10), (4, 4), (9, 10), (10, 9)] {
        assert_eq!(
            scan(&ctx, from, end),
            FontificationScanAdmission::UseGeneric
        );
    }
    ctx.eval_str("(narrow-to-region 4 4)").unwrap();
    assert_eq!(scan(&ctx, 4, 4), FontificationScanAdmission::UseGeneric);
}

#[test]
fn fontification_scan_refuses_every_resolver_fallback_and_overlay() {
    for setup in [
        "(progn (put 'fs-cat 'fontified t) (put-text-property 2 5 'category 'fs-cat))",
        "(put-text-property 2 5 'category nil)",
        "(progn (put-text-property 2 5 'category 'fs-cat) (remove-text-properties 2 5 '(category nil)))",
        "(progn (set (make-local-variable 'char-property-alias-alist) '((fontified fs-alias))) (put-text-property 2 5 'fs-alias t))",
        "(set (make-local-variable 'default-text-properties) '(fontified t))",
        "(set (make-local-variable 'default-text-properties) '(fontified nil))",
        "(let ((o (make-overlay 1 3))) (overlay-put o 'fontified t))",
        "(let ((o (make-overlay 2 5))) (overlay-put o 'face 'bold))",
        "(let ((o (make-overlay 2 5))) (overlay-put o 'fontified nil))",
    ] {
        let mut ctx = Context::new();
        ctx.eval_str("(insert \"abcdefgh\")").unwrap();
        ctx.eval_str(setup).expect("refused source setup");
        assert_eq!(
            scan(&ctx, 1, 9),
            FontificationScanAdmission::UseGeneric,
            "{setup}"
        );
    }
}

#[test]
fn fontification_scan_reacquires_live_state_after_source_changes() {
    let mut ctx = Context::new();
    ctx.eval_str("(insert \"abcdefgh\")").unwrap();
    assert_eq!(
        scan(&ctx, 1, 9),
        FontificationScanAdmission::Raw(Some(LispCharPos1::ONE))
    );
    ctx.eval_str("(put-text-property 1 9 'fontified t)")
        .unwrap();
    assert_eq!(scan(&ctx, 1, 9), FontificationScanAdmission::Raw(None));
    ctx.eval_str("(put-text-property 4 5 'fontified nil)")
        .unwrap();
    assert_eq!(
        scan(&ctx, 1, 9),
        FontificationScanAdmission::Raw(Some(LispCharPos1::new(4)))
    );
    ctx.eval_str("(narrow-to-region 2 7)").unwrap();
    assert_eq!(scan(&ctx, 1, 9), FontificationScanAdmission::UseGeneric);
    assert_raw(&ctx, 2, 7);
    ctx.eval_str("(let ((o (make-overlay 2 3))) (overlay-put o 'fontified t))")
        .unwrap();
    assert_eq!(scan(&ctx, 2, 7), FontificationScanAdmission::UseGeneric);
}
