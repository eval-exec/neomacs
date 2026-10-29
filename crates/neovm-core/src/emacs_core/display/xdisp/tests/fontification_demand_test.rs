use super::*;
use crate::emacs_core::eval::Context;

fn scene(setup: &str) -> (Context, BufferId) {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(insert \"abcdefghij\") (setq ttykey-demand-calls nil ttykey-demand-source-name (buffer-name))").unwrap();
    eval.eval_str(setup).unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    (eval, buffer)
}

fn chars(start: usize, end: usize) -> CharRange {
    CharRange::from_start_len(CharPos0::new(start), CharLen::new(end - start))
}

fn observed(
    eval: &mut Context,
    buffer: BufferId,
    start: i64,
    end: i64,
) -> (RedisplayFontificationOutcome, RedisplayFontificationWitness) {
    let mut demand = RedisplayFontificationDemand::new();
    let outcome = ensure_fontified_for_redisplay_with_demand(eval, buffer, start, end, &mut demand)
        .expect("canonical observed walk");
    (outcome, demand.finish())
}

const NO_MARK_HOOK: &str = r#"
(setq fontification-functions
  (list (lambda (pos) (setq ttykey-demand-calls (cons pos ttykey-demand-calls)))))
"#;

#[test]
fn truthy_and_defer_certify_exact_clamped_character_extent_without_callbacks() {
    for value in ["t", "'defer", "\"truthy\""] {
        let (mut eval, buffer) = scene(NO_MARK_HOOK);
        eval.eval_str(&format!(
            "(put-text-property 1 (point-max) 'fontified {value})"
        ))
        .unwrap();
        let (outcome, witness) = observed(&mut eval, buffer, -100, i64::MAX);
        assert_eq!(outcome, RedisplayFontificationOutcome::Unchanged);
        let RedisplayFontificationWitness::NoDemand(coverage) = witness else {
            panic!("all effective values are non-nil: {witness:?}");
        };
        assert!(coverage.covers(buffer, chars(0, 10)));
        assert!(
            !coverage.covers(buffer, chars(0, 11)),
            "EOB is not source text"
        );
        assert_eq!(
            coverage.spans().collect::<Vec<_>>(),
            vec![(buffer, chars(0, 10))]
        );
        assert!(eval.eval_str("ttykey-demand-calls").unwrap().is_nil());
    }
}

#[test]
fn nil_islands_preserve_legacy_callback_positions_even_when_callbacks_leave_nil() {
    let setup = format!(
        "{NO_MARK_HOOK} (put-text-property 1 (point-max) 'fontified t) \
        (put-text-property 2 4 'fontified nil) (put-text-property 7 9 'fontified nil)"
    );
    let mut traces = Vec::new();
    for observe in [false, true] {
        let (mut eval, buffer) = scene(&setup);
        let outcome = if observe {
            let (outcome, witness) = observed(&mut eval, buffer, 0, 10);
            assert!(matches!(
                witness,
                RedisplayFontificationWitness::CallbacksRan
            ));
            outcome
        } else {
            ensure_fontified_for_redisplay(&mut eval, buffer, 0, 10).unwrap()
        };
        assert_eq!(outcome, RedisplayFontificationOutcome::CallbacksUnchanged);
        traces.push(crate::emacs_core::print::print_value(
            &eval.eval_str("ttykey-demand-calls").unwrap(),
        ));
    }
    assert_eq!(traces, vec!["(8 7 3 2)", "(8 7 3 2)"]);
}

#[test]
fn actual_error_and_switching_callbacks_are_observed_and_source_restoration_is_unchanged() {
    for body in [
        "(signal 'error '(\"intentional fontifier error\"))",
        "(set-buffer (get-buffer-create \"*ttykey-demand-switch*\"))",
    ] {
        let setup = format!(
            r#"
        (setq fontification-functions
          (list (lambda (pos) (setq ttykey-demand-calls (cons pos ttykey-demand-calls)) {body})
                (lambda (pos) (setq ttykey-demand-calls (cons pos ttykey-demand-calls))
                  (save-current-buffer
                    (set-buffer ttykey-demand-source-name)
                    (put-text-property 1 (point-max) 'fontified t)))))
        "#
        );
        let mut traces = Vec::new();
        for observe in [false, true] {
            let (mut eval, buffer) = scene(&setup);
            let saved = eval
                .buffer_manager_mut()
                .create_buffer("*ttykey-demand-saved*");
            eval.set_current_buffer_unrecorded(saved).unwrap();
            let outcome = if observe {
                let (outcome, witness) = observed(&mut eval, buffer, 0, 10);
                assert!(matches!(
                    witness,
                    RedisplayFontificationWitness::CallbacksRan
                ));
                outcome
            } else {
                ensure_fontified_for_redisplay(&mut eval, buffer, 0, 10).unwrap()
            };
            assert_eq!(outcome, RedisplayFontificationOutcome::Fontified);
            assert_eq!(eval.buffer_manager().current_buffer_id(), Some(saved));
            traces.push(crate::emacs_core::print::print_value(
                &eval.eval_str("ttykey-demand-calls").unwrap(),
            ));
        }
        assert_eq!(traces, vec!["(1 1)", "(1 1)"]);
    }
}

#[test]
fn sparse_requests_preserve_gaps_and_refuse_empty_eob_requests() {
    let (mut eval, buffer) = scene(NO_MARK_HOOK);
    eval.eval_str("(put-text-property 1 (point-max) 'fontified t)")
        .unwrap();
    let mut demand = RedisplayFontificationDemand::new();
    for (start, end) in [(0, 2), (7, 10)] {
        ensure_fontified_for_redisplay_with_demand(&mut eval, buffer, start, end, &mut demand)
            .unwrap();
    }
    let RedisplayFontificationWitness::NoDemand(coverage) = demand.finish() else {
        panic!("complete spans");
    };
    assert!(coverage.covers(buffer, chars(0, 2)));
    assert!(coverage.covers(buffer, chars(7, 10)));
    assert!(!coverage.covers(buffer, chars(0, 10)));
    let (_, witness) = observed(&mut eval, buffer, 10, i64::MAX);
    assert!(matches!(witness, RedisplayFontificationWitness::Unknown));
}

#[test]
fn nil_hooks_memory_full_and_overlays_never_construct_positive_witness() {
    for refusal in [
        "(setq fontification-functions nil)",
        "(setq memory-full t)",
        "(make-overlay 1 2)",
    ] {
        let (mut eval, buffer) = scene(NO_MARK_HOOK);
        eval.eval_str("(put-text-property 1 (point-max) 'fontified t)")
            .unwrap();
        eval.eval_str(refusal).unwrap();
        let (_, witness) = observed(&mut eval, buffer, 0, 10);
        assert!(
            matches!(witness, RedisplayFontificationWitness::Unknown),
            "{refusal}: {witness:?}"
        );
    }
}

#[test]
fn no_request_retry_abort_and_span_budget_cannot_publish_coverage() {
    assert!(matches!(
        RedisplayFontificationDemand::new().finish(),
        RedisplayFontificationWitness::Unknown
    ));
    let (mut eval, buffer) = scene(NO_MARK_HOOK);
    eval.eval_str("(put-text-property 1 (point-max) 'fontified t)")
        .unwrap();
    let mut aborted = RedisplayFontificationDemand::new();
    ensure_fontified_for_redisplay_with_demand(&mut eval, buffer, 0, 10, &mut aborted).unwrap();
    aborted.abort();
    assert!(matches!(
        aborted.finish(),
        RedisplayFontificationWitness::Aborted
    ));
    let mut budget = RedisplayFontificationDemand::new();
    for _ in 0..65 {
        ensure_fontified_for_redisplay_with_demand(&mut eval, buffer, 0, 1, &mut budget).unwrap();
    }
    assert!(matches!(
        budget.finish(),
        RedisplayFontificationWitness::Unknown
    ));
}

#[test]
fn first_and_last_nil_stops_are_callbacks_and_eob_is_not_source_coverage() {
    let setup = format!(
        "{NO_MARK_HOOK} (put-text-property 1 (point-max) 'fontified t) \
        (put-text-property 1 2 'fontified nil) (put-text-property 10 11 'fontified nil)"
    );
    let mut traces = Vec::new();
    for observe in [false, true] {
        let (mut eval, buffer) = scene(&setup);
        let outcome = if observe {
            let (outcome, witness) = observed(&mut eval, buffer, 0, i64::MAX);
            assert!(matches!(
                witness,
                RedisplayFontificationWitness::CallbacksRan
            ));
            outcome
        } else {
            ensure_fontified_for_redisplay(&mut eval, buffer, 0, i64::MAX).unwrap()
        };
        assert_eq!(outcome, RedisplayFontificationOutcome::CallbacksUnchanged);
        traces.push(crate::emacs_core::print::print_value(
            &eval.eval_str("ttykey-demand-calls").unwrap(),
        ));
    }
    assert_eq!(traces, vec!["(10 1)", "(10 1)"]);
}

#[test]
fn local_nil_shadows_default_and_local_t_expands_default_without_observer_changes() {
    let mut traces = Vec::new();
    for observe in [false, true] {
        let (mut eval, buffer) = scene(
            r#"
            (set-default 'fontification-functions
              (list (lambda (pos) (setq ttykey-demand-calls (cons pos ttykey-demand-calls)))))
            (make-local-variable 'fontification-functions)
            (setq fontification-functions nil)
        "#,
        );
        let mut demand = RedisplayFontificationDemand::new();
        let outcome = if observe {
            ensure_fontified_for_redisplay_with_demand(&mut eval, buffer, 0, 1, &mut demand)
                .unwrap()
        } else {
            ensure_fontified_for_redisplay(&mut eval, buffer, 0, 1).unwrap()
        };
        assert_eq!(outcome, RedisplayFontificationOutcome::Unchanged);
        if observe {
            assert!(matches!(
                demand.finish(),
                RedisplayFontificationWitness::Unknown
            ));
        }
        assert!(eval.eval_str("ttykey-demand-calls").unwrap().is_nil());
        eval.eval_str("(setq fontification-functions '(t))")
            .unwrap();
        let outcome = if observe {
            let (outcome, witness) = observed(&mut eval, buffer, 0, 1);
            assert!(matches!(
                witness,
                RedisplayFontificationWitness::CallbacksRan
            ));
            outcome
        } else {
            ensure_fontified_for_redisplay(&mut eval, buffer, 0, 1).unwrap()
        };
        assert_eq!(outcome, RedisplayFontificationOutcome::CallbacksUnchanged);
        traces.push(crate::emacs_core::print::print_value(
            &eval.eval_str("ttykey-demand-calls").unwrap(),
        ));
    }
    assert_eq!(traces, vec!["(1)", "(1)"]);
}

#[test]
fn observer_preserves_both_existing_explicit_boundary_modes() {
    for mode in [
        FontificationBoundaryMode::Off,
        FontificationBoundaryMode::On,
    ] {
        for setup in [
            format!("{NO_MARK_HOOK} (put-text-property 1 (point-max) 'fontified 'defer)"),
            format!("{NO_MARK_HOOK} (put-text-property 1 4 'fontified t)"),
        ] {
            let (mut plain, source) = scene(&setup);
            let expected =
                ensure_fontified_for_redisplay_with_mode(&mut plain, source, 0, 10, mode).unwrap();
            let expected_calls = crate::emacs_core::print::print_value(
                &plain.eval_str("ttykey-demand-calls").unwrap(),
            );
            drop(plain);
            let (mut observed_ctx, source) = scene(&setup);
            let mut demand = RedisplayFontificationDemand::new();
            let actual = ensure_fontified_for_redisplay_inner(
                &mut observed_ctx,
                source,
                0,
                10,
                mode,
                &mut demand,
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                crate::emacs_core::print::print_value(
                    &observed_ctx.eval_str("ttykey-demand-calls").unwrap()
                ),
                expected_calls
            );
            let witness = demand.finish();
            assert!(
                matches!(witness, RedisplayFontificationWitness::NoDemand(_))
                    || matches!(witness, RedisplayFontificationWitness::CallbacksRan)
            );
        }
    }
}

// These are the same resolved-property sources as fontification_boundary's
// fallback fixtures. This witnesses one canonical walk, not future validity
// of the category plist or the alias/default variable values.
fn assert_resolved_truthy_demand_observation(setup: &str) {
    for mode in [
        FontificationBoundaryMode::Off,
        FontificationBoundaryMode::On,
    ] {
        let mut observations = Vec::new();
        for observe in [false, true] {
            let (mut eval, buffer) = scene(NO_MARK_HOOK);
            eval.eval_str(setup).expect("effective non-nil fallback");
            assert!(
                eval.eval_str(
                    "(and (get-char-property 1 'fontified)
                          (get-char-property 10 'fontified))"
                )
                .unwrap()
                .is_truthy(),
                "both ends must use an effective non-nil fallback: {setup}"
            );
            let outcome = if observe {
                let mut demand = RedisplayFontificationDemand::new();
                let outcome = ensure_fontified_for_redisplay_inner(
                    &mut eval,
                    buffer,
                    0,
                    10,
                    mode,
                    &mut demand,
                )
                .expect("observed resolved fallback walk");
                let witness = demand.finish();
                let RedisplayFontificationWitness::NoDemand(coverage) = witness else {
                    panic!("effective truthy values require no callback: {setup}: {witness:?}");
                };
                assert!(coverage.covers(buffer, chars(0, 10)));
                assert!(!coverage.covers(buffer, chars(0, 11)));
                outcome
            } else {
                ensure_fontified_for_redisplay_with_mode(&mut eval, buffer, 0, 10, mode)
                    .expect("legacy resolved fallback walk")
            };
            assert_eq!(outcome, RedisplayFontificationOutcome::Unchanged);
            let calls = eval.eval_str("ttykey-demand-calls").unwrap();
            assert!(calls.is_nil(), "fallback must suppress callbacks: {setup}");
            observations.push((outcome, crate::emacs_core::print::print_value(&calls)));
        }
        assert_eq!(observations[0], observations[1], "{mode:?}: {setup}");
    }
}

#[test]
fn category_truthy_values_produce_observed_no_demand_in_both_boundary_modes() {
    assert_resolved_truthy_demand_observation(
        "(put 'ttykey-demand-category 'fontified t)
         (put-text-property 1 (point-max) 'category 'ttykey-demand-category)",
    );
}

#[test]
fn default_truthy_values_produce_observed_no_demand_in_both_boundary_modes() {
    assert_resolved_truthy_demand_observation(
        "(set (make-local-variable 'default-text-properties) '(fontified t))",
    );
}

#[test]
fn alias_truthy_values_produce_observed_no_demand_in_both_boundary_modes() {
    assert_resolved_truthy_demand_observation(
        "(set (make-local-variable 'char-property-alias-alist)
              '((fontified ttykey-demand-alias)))
         (put-text-property 1 (point-max) 'ttykey-demand-alias t)",
    );
}

#[test]
fn actual_source_killing_callback_preserves_legacy_outcome_and_never_no_demand() {
    for mode in [
        FontificationBoundaryMode::Off,
        FontificationBoundaryMode::On,
    ] {
        let mut observations = Vec::new();
        for observe in [false, true] {
            let (mut eval, source) = scene(
                "(setq kill-buffer-hook nil kill-buffer-query-functions nil
                       fontification-functions
                       (list (lambda (pos)
                         (setq ttykey-demand-calls (cons pos ttykey-demand-calls))
                         (kill-buffer (current-buffer)))))",
            );
            let saved = eval
                .buffer_manager_mut()
                .create_buffer("*ttykey-demand-saved*");
            eval.set_current_buffer_unrecorded(saved).unwrap();
            let mut demand = RedisplayFontificationDemand::new();
            let result = if observe {
                let result = ensure_fontified_for_redisplay_inner(
                    &mut eval,
                    source,
                    0,
                    10,
                    mode,
                    &mut demand,
                );
                // Match the public observed wrapper's error ownership.
                if result.is_err() {
                    demand.abort();
                }
                result
            } else {
                ensure_fontified_for_redisplay_with_mode(&mut eval, source, 0, 10, mode)
            };
            assert!(
                eval.buffer_manager().get(source).is_none(),
                "the actual hook must kill its source"
            );
            assert_eq!(eval.buffer_manager().current_buffer_id(), Some(saved));
            if observe {
                let witness = demand.finish();
                assert!(
                    !matches!(witness, RedisplayFontificationWitness::NoDemand(_)),
                    "a killed source cannot publish positive evidence: {witness:?}"
                );
            }
            let outcome = match result {
                Ok(outcome) => format!("{outcome:?}"),
                Err(flow) => crate::emacs_core::print::format_flow_with_eval(&mut eval, &flow),
            };
            let calls = crate::emacs_core::print::print_value(
                &eval.eval_str("ttykey-demand-calls").unwrap(),
            );
            assert_eq!(calls, "(1)", "the real killing hook must run exactly once");
            observations.push((outcome, calls));
        }
        assert_eq!(observations[0], observations[1], "{mode:?}");
    }
}
