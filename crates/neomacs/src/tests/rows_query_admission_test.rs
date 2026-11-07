use super::*;
use neomacs_layout_engine::RowsQueryAdmission;
use neovm_core::emacs_core::eval::{RowsObservationEpoch, RowsObservationRefusal};

fn parse(args: &[&str]) -> Result<StartupOptions, String> {
    parse_startup_options(args.iter().map(|arg| (*arg).to_owned()))
}

/// Pin the collector mode through value's signed per-heap test fence. The
/// canonical Rows producers require GEN1; the GEN0 twins assert the typed
/// refusal instead. Neither configuration is inherited from the test runner.
fn pin_gc_mode(context: &mut Context, generational: bool) {
    // SAFETY: each test owns this fresh Context and pins the heap before any
    // Layout query; no collector or compiled leaf runs across the switch.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            context.tagged_heap_mut(),
            generational,
        );
    }
    assert_eq!(
        neovm_value::test_support::authority::generational_enabled_for_test(context.tagged_heap()),
        generational,
        "the Rows query fixture must run under its explicitly pinned GC mode",
    );
}

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        neovm_core::emacs_core::terminal::pure::reset_terminal_host();
        neovm_core::emacs_core::terminal::pure::reset_terminal_runtime();
    }
}

/// Build the actual TTY host authority and request the observation. The caller
/// pins GEN1 (canonical) or GEN0 (typed refusal twin) before entering.
fn begin_actual_tty_rows_observation(
    context: &mut Context,
) -> Result<RowsObservationEpoch, RowsObservationRefusal> {
    use neovm_core::emacs_core::DisplayHost;
    use neovm_core::emacs_core::terminal::pure::{self, TerminalRuntimeConfig};
    use std::sync::atomic::{AtomicBool, Ordering};

    context.setup_thread_locals();
    pure::configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let source = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frame_manager_mut()
        .create_frame("actual-tty-host", 120, 40, source);
    context.sync_runtime_faces_for_frame(frame);
    let redraw = std::sync::Arc::new(AtomicBool::new(false));
    let host = tty_frontend::TtyPopupDisplayHost::new(redraw.clone());
    assert!(host.image_catalog().is_none());
    assert!(host.image_catalog_shared().is_none());
    context.set_display_host(Box::new(host));
    let authority = frame_layout::tty_rows_host_authority();
    let observation = context.begin_tty_rows_observation(frame, Some(&authority));
    if observation.is_ok() {
        redraw.store(true, Ordering::Release);
    }
    observation
}

/// Run the canonical numeric TTY Rows producer and report the geometry shape
/// plus the exact post-proof report it recorded. The caller pins the GC mode.
fn canonical_tty_rows_lease_case(context: &mut Context) -> (usize, usize, Vec<String>) {
    use neomacs_layout_engine::{LayoutEngine, RowsQueryPolicy};
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::emacs_core::terminal::pure::{self, TerminalRuntimeConfig};
    use neovm_core::window::WindowLayoutQueryScope;
    use std::num::NonZeroUsize;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use tracing::{Event, Metadata, Subscriber, field, span, subscriber::Interest};

    // Only the existing post-proof report is recorded. No private Layout API,
    // forged seal, global subscriber, new TLS or engine field is involved.
    struct Reports(Arc<Mutex<Vec<String>>>);
    struct ReportVisitor<'a>(&'a mut Vec<String>);
    impl field::Visit for ReportVisitor<'_> {
        fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "report" {
                self.0.push(format!("{value:?}"));
            }
        }
    }
    impl Subscriber for Reports {
        fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
            // Recheck against this scoped subscriber, including callsites
            // registered earlier by another test or the process subscriber.
            Interest::sometimes()
        }
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.is_event()
                && metadata.target() == "neomacs_layout_engine::query_cache"
                && metadata.fields().field("report").is_some()
        }
        fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
            Some(tracing::level_filters::LevelFilter::TRACE)
        }
        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            // All spans are disabled; no span identity or storage is needed.
            span::Id::from_u64(1)
        }
        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
        fn enter(&self, _: &span::Id) {}
        fn exit(&self, _: &span::Id) {}
        fn event(&self, event: &Event<'_>) {
            // Be defensive if a shared callsite is enabled by another dispatch.
            if self.enabled(event.metadata()) {
                let mut reports = self.0.lock().unwrap();
                event.record(&mut ReportVisitor(&mut reports));
            }
        }
    }

    // Use public collection APIs, matching the actual Layout TTY fixture.
    context.set_gc_threshold(usize::MAX);
    context.gc_collect_exact();
    pure::configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let source = context.buffer_manager().current_buffer().unwrap().id();
    let text: String = (0..1_000)
        .map(|line| format!("{line:04} numeric tty row\n"))
        .collect();
    context
        .buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert(&text);
    let frame =
        context
            .frame_manager_mut()
            .create_frame("actual-tty-certified-rows", 120, 40, source);
    assert!(context.frame_manager_mut().select_frame(frame));
    assert!(context.frame_is_tty_face_target(frame));
    let window = context.frame_manager().get(frame).unwrap().selected_window;
    context
        .eval_str(
            r#"(setq mode-line-format nil header-line-format nil tab-line-format nil
          fontification-functions (list 'ignore) glyphless-char-display nil
          buffer-display-table nil standard-display-table nil char-width-table nil)
          (put-text-property (point-min) (point-max) 'fontified t)
          (goto-char 1) (set-window-start nil 1 t)"#,
        )
        .unwrap();
    context.set_display_host(Box::new(tty_frontend::TtyPopupDisplayHost::new(Arc::new(
        AtomicBool::new(false),
    ))));
    context.sync_runtime_faces_for_frame(frame);
    context.setup_thread_locals();
    // Obtain the real frontend's audited authority, not a new test host type.
    // Off still observes the real producer; it cannot grant a cached hit.
    let policy = RowsQueryPolicy::terminal(
        RowsQueryAdmission::Off,
        frame_layout::tty_rows_host_authority(),
    );
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut engine = LayoutEngine::new_without_font_metrics();
    let query = tracing::subscriber::with_default(Reports(reports.clone()), || {
        engine
            .query_window_layout_with_policy(
                context,
                frame,
                window,
                WindowLayoutQueryScope::Rows {
                    start: LispCharPos1::ONE,
                    count: NonZeroUsize::new(40).unwrap(),
                },
                &policy,
            )
            .expect("the real frontend host must permit the canonical numeric TTY Rows query")
    });
    let geometry = query
        .geometry()
        .expect("the canonical query must return real geometry");
    assert!(!geometry.rows.is_empty());
    assert!(geometry.point_count() > 0);
    let rows = geometry.rows.len();
    let points = geometry.point_count();
    // finish_and_drop releases the source Ref before this trace and return.
    // The public mutable buffer access must remain possible afterwards.
    context
        .buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert("after real TTY certificate");
    context.gc_collect_exact();
    let reports = reports.lock().unwrap().clone();
    (rows, points, reports)
}

#[test]
fn rows_query_admission_defaults_exact_proven_and_preserves_typed_opt_out() {
    let default = parse(&["neomacs", "-Q", "-nw"]).unwrap();
    assert_eq!(
        RowsQueryAdmission::default(),
        RowsQueryAdmission::ExactProven
    );
    assert_eq!(
        default.rows_query_admission,
        RowsQueryAdmission::ExactProven
    );
    assert_eq!(
        neomacs_layout_engine::RowsQueryPolicy::default().admission(),
        RowsQueryAdmission::Off,
    );
    assert!(render_help_text("neomacs").contains(
        "--rows-query-admission=MODE  TTY Rows reuse: exact-proven (default) or off",
    ));
    for (args, admission) in [
        (
            vec![
                "neomacs",
                "-nw",
                "--rows-query-admission=exact-proven",
                "file.txt",
            ],
            RowsQueryAdmission::ExactProven,
        ),
        (
            vec![
                "neomacs",
                "file.txt",
                "--rows-query-admission",
                "exact-proven",
                "-nw",
            ],
            RowsQueryAdmission::ExactProven,
        ),
        (
            vec!["neomacs", "-nw", "--rows-query-admission=off", "file.txt"],
            RowsQueryAdmission::Off,
        ),
        (
            vec![
                "neomacs",
                "file.txt",
                "--rows-query-admission",
                "off",
                "-nw",
            ],
            RowsQueryAdmission::Off,
        ),
    ] {
        let selected = parse(&args).unwrap();
        assert_eq!(selected.rows_query_admission, admission);
        assert!(!selected.forwarded_args.iter().any(|arg| {
            arg.contains("rows-query-admission") || arg == "exact-proven" || arg == "off"
        }));
        assert!(selected.forwarded_args.iter().any(|arg| arg == "file.txt"));
    }
}

#[test]
fn rows_query_admission_preserves_option_looking_eval_operand_and_terminator() {
    let operand = "--rows-query-admission=off";
    for switch in ["--eval", "-eval", "--execute"] {
        let parsed = parse(&["neomacs", "-nw", switch, operand]).unwrap();
        assert_eq!(parsed.rows_query_admission, RowsQueryAdmission::ExactProven);
        assert!(
            parsed
                .forwarded_args
                .windows(2)
                .any(|args| args == [switch, operand])
        );
    }
    let parsed = parse(&["neomacs", "-nw", "--", operand]).unwrap();
    assert_eq!(parsed.rows_query_admission, RowsQueryAdmission::ExactProven);
    assert_eq!(parsed.forwarded_args.last().unwrap(), operand);
}

#[test]
fn rows_query_admission_rejects_unknown_or_missing_value() {
    for args in [
        vec!["neomacs", "--rows-query-admission=on"],
        vec!["neomacs", "--rows-query-admission="],
        vec!["neomacs", "--rows-query-admission"],
    ] {
        assert!(parse(&args).is_err());
    }
    let parsed = parse(&[
        "neomacs",
        "--rows-query-admission=exact-proven",
        "--rows-query-admission=off",
    ])
    .unwrap();
    assert_eq!(parsed.rows_query_admission, RowsQueryAdmission::Off);
}

#[test]
fn rows_query_admission_actual_tty_host_matches_inert_type_authority() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    pin_gc_mode(&mut context, true);
    let _restore = RestoreTerminal;
    let epoch = begin_actual_tty_rows_observation(&mut context)
        .expect("GEN1 must admit the TTY Rows observation");
    // The host redraw flag was armed when the observation was admitted.
    assert!(context.static_rows_observation_current(&epoch).is_ok());
    // This establishes actual frontend host authority, not a Layout producer
    // certificate or eligibility of the loaded keystroke scene.
}

#[test]
fn rows_query_admission_gen0_tty_host_refuses_typed_generational_disabled() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    pin_gc_mode(&mut context, false);
    let _restore = RestoreTerminal;
    let refusal = begin_actual_tty_rows_observation(&mut context)
        .expect_err("GEN0 must refuse the TTY Rows observation");
    assert_eq!(refusal, RowsObservationRefusal::GenerationalDisabled);
}

#[test]
fn rows_query_admission_actual_tty_host_canonical_rows_certifies_and_drops_lease() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    pin_gc_mode(&mut context, true);
    let _restore = RestoreTerminal;
    let (rows, points, reports) = canonical_tty_rows_lease_case(&mut context);
    assert_eq!(
        reports,
        vec![format!("Certified {{ rows: {rows}, points: {points} }}")],
        "exactly one real canonical producer must close all fontification, face-entry, source and dependency proofs",
    );
}

#[test]
fn rows_query_admission_gen0_canonical_rows_records_typed_generational_refusal() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    pin_gc_mode(&mut context, false);
    let _restore = RestoreTerminal;
    let (rows, points, reports) = canonical_tty_rows_lease_case(&mut context);
    assert!(rows > 0 && points > 0);
    assert_eq!(
        reports,
        vec!["Refused(Core(GenerationalDisabled))".to_owned()],
        "GEN0 has no partial/error collector refusal to offer the Rows producer, so the canonical query must record exactly the typed refusal",
    );
}
