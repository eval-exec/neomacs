//! Deterministic tests for the pure frame scheduler. No window, GPU, or
//! wall-clock dependency: one observation anchors a synthetic timeline and all
//! other times are derived from it by `Duration` arithmetic.

use super::*;

fn win(n: u64) -> NativeWindowId {
    NativeWindowId(n)
}

fn t0() -> EventTime {
    neomacs_display_protocol::frame_time::observe_platform_now()
}

fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}

fn tick_at(at: EventTime) -> FrameTick {
    FrameTick {
        frame_time: at,
        target_presentation_time: at.plus(ms(8)),
        estimated_interval: ms(16),
        source: ClockSource::Synthetic,
    }
}

fn composite_cursor() -> FrameDemand {
    FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::NextPresentation,
        reason: DemandReason::CursorAnimation,
    }
}

fn editor_commit() -> FrameDemand {
    FrameDemand {
        invalidation: Invalidation::RebuildScene,
        cadence: Cadence::NextPresentation,
        reason: DemandReason::EditorCommit,
    }
}

#[test]
fn no_demand_means_sleep_and_no_deadline() {
    let mut c = FrameCoordinator::new();
    assert_eq!(c.next_wake_deadline(), None);
    // A platform-initiated tick with no demand plans no work.
    let plan = c.begin_frame(win(1), tick_at(t0()));
    assert_eq!(plan.work, RenderWork::None);
    assert!(!plan.should_present);
}

#[test]
fn duplicate_demand_coalesces_into_one_request() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    assert_eq!(
        c.submit_demand(win(1), composite_cursor(), now),
        PacingAction::RequestRedraw
    );
    for _ in 0..9 {
        assert_eq!(
            c.submit_demand(win(1), composite_cursor(), now),
            PacingAction::Sleep
        );
    }
    assert!(c.request_pending(win(1)));
    // The tick consumes the request; a new demand may request again.
    let _ = c.begin_frame(win(1), tick_at(now.plus(ms(5))));
    assert!(!c.request_pending(win(1)));
    assert_eq!(
        c.submit_demand(win(1), composite_cursor(), now.plus(ms(6))),
        PacingAction::RequestRedraw
    );
}

#[test]
fn editor_commit_dominates_compositor_only_demand() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    c.submit_demand(win(1), editor_commit(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert_eq!(plan.work, RenderWork::RebuildScene);
    assert!(plan.should_present);
}

#[test]
fn composite_layers_union() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::MEDIA,
            },
            cadence: Cadence::NextPresentation,
            reason: DemandReason::Video,
        },
        now,
    );
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert_eq!(
        plan.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS | LayerMask::MEDIA,
        }
    );
}

#[test]
fn frame_shader_clock_recomposites_without_rebuilding_the_scene() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::FRAME_POST,
            },
            cadence: Cadence::MaxRate(std::num::NonZeroU16::new(60).unwrap()),
            reason: DemandReason::FrameShader,
        },
        now,
    );

    let plan = c.begin_frame(win(1), tick_at(now));
    assert_eq!(
        plan.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::FRAME_POST,
        }
    );
    assert!(plan.reasons.contains(DemandReason::FrameShader));
}

#[test]
fn earliest_deadline_wins() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(500))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::TRANSIENT_OVERLAYS,
            },
            cadence: Cadence::At(now.plus(ms(200))),
            reason: DemandReason::FiniteEffect,
        },
        now,
    );
    assert_eq!(c.next_wake_deadline(), Some(now.plus(ms(200))));
}

#[test]
fn ripe_video_deadline_requests_service_without_creating_frame_work() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let video_at = now.plus(ms(100));
    let frame_at = now.plus(ms(200));
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(frame_at),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    c.reconcile_video_service_deadline(Some(video_at));

    assert_eq!(
        c.service_deadlines(now).wake,
        LoopWake::At(FutureDeadline(video_at))
    );

    let due = c.service_deadlines(video_at);
    assert!(
        due.video_service_due,
        "a ripe media deadline must request another decoder service pass"
    );
    assert!(due.redraw.is_empty());
    assert_eq!(due.wake, LoopWake::At(FutureDeadline(frame_at)));
    assert_eq!(
        c.begin_frame(win(1), tick_at(video_at)).work,
        RenderWork::None,
        "a decoder-service wake is not itself permission to repaint"
    );

    // The service pass replaces the consumed deadline with its next future
    // wake. Reconciliation must converge without manufacturing frame work.
    let next_video_at = video_at.plus(ms(50));
    c.reconcile_video_service_deadline(Some(next_video_at));
    let reconciled = c.service_deadlines(video_at);
    assert!(!reconciled.video_service_due);
    assert!(reconciled.redraw.is_empty());
    assert_eq!(reconciled.wake, LoopWake::At(FutureDeadline(next_video_at)));
}

#[test]
fn a_ready_video_frame_becomes_media_damage_before_sleep() {
    let mut c = FrameCoordinator::new();
    let now = t0();

    assert_eq!(
        c.submit_ready_video_frame(win(1), now),
        PacingAction::RequestRedraw
    );
    let plan = c.begin_frame(win(1), tick_at(now));
    assert_eq!(
        plan.work,
        RenderWork::RepaintLayers {
            layers: LayerMask::MEDIA,
            damage: Damage::FullLayer,
        }
    );
    assert!(plan.reasons.contains(DemandReason::Video));
}

#[test]
fn deadline_not_consumed_before_it_is_ripe() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(500))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    let early = c.begin_frame(win(1), tick_at(now.plus(ms(100))));
    assert_eq!(early.work, RenderWork::None);
    assert_eq!(c.next_wake_deadline(), Some(now.plus(ms(500))));
    let ripe = c.begin_frame(win(1), tick_at(now.plus(ms(500))));
    assert_eq!(
        ripe.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        }
    );
    assert_eq!(c.next_wake_deadline(), None);
}

#[test]
fn late_tick_consumes_backlog_as_one_plan() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(100))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::RepaintLayers {
                layers: LayerMask::CHROME,
                damage: Damage::FullLayer,
            },
            cadence: Cadence::At(now.plus(ms(200))),
            reason: DemandReason::Transition,
        },
        now,
    );
    // The tick arrives far after both deadlines: one plan, no backlog left.
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(5000))));
    assert_eq!(
        plan.work,
        RenderWork::RepaintLayers {
            layers: LayerMask::CHROME,
            damage: Damage::FullLayer,
        }
    );
    assert_eq!(c.next_wake_deadline(), None);
    let next = c.begin_frame(win(1), tick_at(now.plus(ms(5016))));
    assert_eq!(next.work, RenderWork::None);
}

#[test]
fn occluded_window_retains_demand_and_does_not_present() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.update_window_state(
        win(1),
        WindowPresentationState {
            visible: true,
            occluded: true,
            focused: false,
        },
    );
    assert_eq!(
        c.submit_demand(win(1), composite_cursor(), now),
        PacingAction::Sleep
    );
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert!(!plan.should_present);
    assert_eq!(plan.work, RenderWork::None);
    // Demand survives occlusion.
    assert_eq!(
        c.active_reasons(win(1)),
        vec![DemandReason::CursorAnimation]
    );
    // Occluded windows contribute no wake deadline.
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(100))),
            reason: DemandReason::FiniteEffect,
        },
        now,
    );
    assert_eq!(c.next_wake_deadline(), None);
}

#[test]
fn exposure_issues_exactly_one_recovery_request() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.update_window_state(
        win(1),
        WindowPresentationState {
            visible: true,
            occluded: true,
            focused: false,
        },
    );
    c.submit_demand(win(1), composite_cursor(), now);
    let exposed = WindowPresentationState {
        visible: true,
        occluded: false,
        focused: true,
    };
    assert_eq!(
        c.update_window_state(win(1), exposed),
        PacingAction::RequestRedraw
    );
    // Reporting the same state again does not request another frame.
    assert_eq!(c.update_window_state(win(1), exposed), PacingAction::Sleep);
}

#[test]
fn max_rate_phase_survives_interleaved_commits() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let rate = NonZeroU16::new(10).unwrap(); // 100 ms period
    let ambient = |c: &mut FrameCoordinator, at: EventTime| {
        c.submit_demand(
            win(1),
            FrameDemand {
                invalidation: Invalidation::CompositeOnly {
                    layers: LayerMask::CURSOR_EFFECTS,
                },
                cadence: Cadence::MaxRate(rate),
                reason: DemandReason::CursorAnimation,
            },
            at,
        )
    };

    // First submission fires immediately and anchors the grid at now+100ms.
    assert_eq!(ambient(&mut c, now), PacingAction::RequestRedraw);
    let _ = c.begin_frame(win(1), tick_at(now.plus(ms(1))));

    // Standing resubmission lands on the anchor.
    assert_eq!(
        ambient(&mut c, now.plus(ms(2))),
        PacingAction::WakeAt(now.plus(ms(100)))
    );

    // An editor commit interleaves at t+30ms and renders its own frame.
    c.submit_demand(win(1), editor_commit(), now.plus(ms(30)));
    let commit_plan = c.begin_frame(win(1), tick_at(now.plus(ms(31))));
    assert_eq!(commit_plan.work, RenderWork::RebuildScene);

    // The ambient deadline is still the original grid point, not 30ms+100ms.
    assert_eq!(c.next_wake_deadline(), Some(now.plus(ms(100))));

    // Consuming the ambient tick advances the anchor by a whole period.
    let ambient_plan = c.begin_frame(win(1), tick_at(now.plus(ms(101))));
    assert_eq!(
        ambient_plan.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        }
    );
    assert_eq!(
        ambient(&mut c, now.plus(ms(102))),
        PacingAction::WakeAt(now.plus(ms(200)))
    );
}

#[test]
fn changing_max_rate_reanchors_without_waiting_for_the_old_period() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let demand = |rate| FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::MaxRate(NonZeroU16::new(rate).unwrap()),
        reason: DemandReason::CursorColorCycle,
    };

    assert_eq!(
        c.submit_demand(win(1), demand(1), now),
        PacingAction::RequestRedraw
    );
    let _ = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert_eq!(
        c.submit_demand(win(1), demand(1), now.plus(ms(2))),
        PacingAction::WakeAt(now.plus(Duration::from_secs(1)))
    );

    let changed_at = now.plus(ms(10));
    assert_eq!(
        c.submit_demand(win(1), demand(24), changed_at),
        PacingAction::RequestRedraw
    );
    let _ = c.begin_frame(win(1), tick_at(changed_at.plus(ms(1))));
    let next = changed_at.plus(Duration::from_secs_f64(1.0 / 24.0));
    assert_eq!(
        c.submit_demand(win(1), demand(24), changed_at.plus(ms(2))),
        PacingAction::WakeAt(next)
    );
    assert_eq!(c.next_wake_deadline(), Some(next));
}

#[test]
fn max_rate_preserves_its_phase_after_a_year_of_missed_frames() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let rate = std::num::NonZeroU16::new(1).unwrap();
    let demand = FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::MaxRate(rate),
        reason: DemandReason::CursorColorCycle,
    };

    c.submit_demand(win(1), demand, now);
    let _ = c.begin_frame(win(1), tick_at(now));

    let resumed_at = now
        .plus(Duration::from_secs(365 * 24 * 60 * 60))
        .plus(Duration::from_millis(250));
    assert_eq!(
        c.submit_demand(win(1), demand, resumed_at),
        PacingAction::RequestRedraw
    );
    let _ = c.begin_frame(win(1), tick_at(resumed_at));
    let next = match c.submit_demand(win(1), demand, resumed_at.plus(Duration::from_nanos(1))) {
        PacingAction::WakeAt(next) => next,
        action => panic!("expected a phase-aligned deadline after resume, got {action:?}"),
    };

    assert_eq!(
        next,
        resumed_at.plus(Duration::from_millis(750)),
        "missed frames must advance on the original phase grid, not re-anchor at resume time"
    );
}

#[test]
fn late_reconciliation_consumes_the_expired_schedule_exactly_once() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let demand = FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::MaxRate(std::num::NonZeroU16::new(1).unwrap()),
        reason: DemandReason::CursorColorCycle,
    };

    assert_eq!(
        c.submit_demand(win(1), demand, now),
        PacingAction::RequestRedraw
    );
    let _ = c.begin_frame(win(1), tick_at(now));
    assert_eq!(
        c.submit_demand(win(1), demand, now.plus(ms(100))),
        PacingAction::WakeAt(now.plus(Duration::from_secs(1)))
    );

    let late = now.plus(Duration::from_millis(1_250));
    assert_eq!(
        c.submit_demand(win(1), demand, late),
        PacingAction::RequestRedraw
    );
    let plan = c.begin_frame(win(1), tick_at(late));
    assert!(plan.reasons.contains(DemandReason::CursorColorCycle));
    assert_eq!(
        c.submit_demand(win(1), demand, late.plus(Duration::from_nanos(1))),
        PacingAction::WakeAt(now.plus(Duration::from_secs(2))),
        "the expired scheduled record must not be consumed twice or re-anchor the phase"
    );
}

#[test]
fn commit_wakes_immediately_despite_future_deadline() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(400))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    // The commit does not wait for the 400ms deadline.
    assert_eq!(
        c.submit_demand(win(1), editor_commit(), now.plus(ms(10))),
        PacingAction::RequestRedraw
    );
}

#[test]
fn windows_are_independent() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    assert!(c.request_pending(win(1)));
    assert!(!c.request_pending(win(2)));
    c.submit_demand(
        win(2),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::MEDIA,
            },
            cadence: Cadence::At(now.plus(ms(300))),
            reason: DemandReason::Video,
        },
        now,
    );
    // Window 1's tick consumes only window 1's demand.
    let plan1 = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert_eq!(
        plan1.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        }
    );
    assert_eq!(c.next_wake_deadline(), Some(now.plus(ms(300))));
    let plan2 = c.begin_frame(win(2), tick_at(now.plus(ms(300))));
    assert_eq!(
        plan2.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::MEDIA,
        }
    );
}

#[test]
fn demand_submitted_during_render_requests_followup() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    // New demand arrives while the frame is being rendered.
    c.submit_demand(win(1), editor_commit(), now.plus(ms(2)));
    // request_pending was set again by the mid-render submit; finish_frame
    // must not request twice but the demand must survive.
    let action = c.finish_frame(win(1), &plan, PresentResult::Presented, now.plus(ms(3)));
    assert_eq!(action, PacingAction::Sleep);
    let next = c.begin_frame(win(1), tick_at(now.plus(ms(17))));
    assert_eq!(next.work, RenderWork::RebuildScene);
}

#[test]
fn timeout_backs_off_instead_of_retrying_immediately() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    let action = c.finish_frame(win(1), &plan, PresentResult::Timeout, now.plus(ms(2)));
    match action {
        PacingAction::WakeAt(at) => assert!(at > now.plus(ms(2))),
        other => panic!("expected bounded backoff, got {:?}", other),
    }
    assert!(!c.request_pending(win(1)));
    // The retry survives as a scheduled deadline and delivers the work.
    let deadline = c.next_wake_deadline().expect("recovery deadline");
    assert!(deadline > now.plus(ms(2)));
    let retry = c.begin_frame(win(1), tick_at(deadline));
    assert_eq!(
        retry.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        }
    );
}

#[test]
fn awaiting_content_sleeps_until_a_content_producer_submits_new_demand() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), editor_commit(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));

    let action = c.finish_frame(
        win(1),
        &plan,
        PresentResult::AwaitingContent,
        now.plus(ms(2)),
    );

    assert_eq!(action, PacingAction::Sleep);
    assert_eq!(c.active_reasons(win(1)), Vec::new());
    assert_eq!(c.next_wake_deadline(), None);
    assert_eq!(
        c.submit_demand(win(1), editor_commit(), now.plus(ms(3))),
        PacingAction::RequestRedraw
    );
}

#[test]
fn surface_lost_requeues_full_repaint() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    let action = c.finish_frame(win(1), &plan, PresentResult::SurfaceLost, now.plus(ms(2)));
    assert_eq!(action, PacingAction::RequestRedraw);
    let next = c.begin_frame(win(1), tick_at(now.plus(ms(10))));
    assert_eq!(
        next.work,
        RenderWork::RepaintLayers {
            layers: LayerMask::all(),
            damage: Damage::FullLayer,
        }
    );
}

#[test]
fn skipped_present_requeues_work() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), editor_commit(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert_eq!(plan.work, RenderWork::RebuildScene);
    let action = c.finish_frame(win(1), &plan, PresentResult::Skipped, now.plus(ms(2)));
    assert_eq!(action, PacingAction::RequestRedraw);
    let next = c.begin_frame(win(1), tick_at(now.plus(ms(17))));
    assert_eq!(next.work, RenderWork::RebuildScene);
}

#[test]
fn retract_withdraws_standing_deadline() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(250))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    assert_eq!(c.next_wake_deadline(), Some(now.plus(ms(250))));
    c.retract(win(1), DemandReason::CursorAnimation);
    assert_eq!(c.next_wake_deadline(), None);
}

#[test]
fn on_demand_folds_into_next_frame_without_driving_one() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    assert_eq!(
        c.submit_demand(
            win(1),
            FrameDemand {
                invalidation: Invalidation::RepaintLayers {
                    layers: LayerMask::CHROME,
                    damage: Damage::FullLayer,
                },
                cadence: Cadence::OnDemand,
                reason: DemandReason::DebugCapture,
            },
            now,
        ),
        PacingAction::Sleep
    );
    assert!(!c.request_pending(win(1)));
    assert_eq!(c.next_wake_deadline(), None);
    // A driving demand arrives; the OnDemand work rides along.
    c.submit_demand(win(1), composite_cursor(), now.plus(ms(1)));
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(2))));
    assert_eq!(
        plan.work,
        RenderWork::RepaintLayers {
            layers: LayerMask::CHROME,
            damage: Damage::FullLayer,
        }
    );
}

#[test]
fn set_occluded_preserves_focus_and_issues_single_recovery() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    // Focus is set first, then occlusion toggles; occlusion must not clobber
    // the recorded focus, and exposure with demand issues one recovery.
    c.set_focused(win(1), false);
    c.submit_demand(win(1), composite_cursor(), now);
    assert_eq!(c.set_occluded(win(1), true), PacingAction::Sleep);
    assert!(!c.is_eligible(win(1)));
    // No wake deadline while occluded even with a scheduled demand.
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(50))),
            reason: DemandReason::FiniteEffect,
        },
        now,
    );
    assert_eq!(c.next_wake_deadline(), None);
    // Exposure issues exactly one recovery request, then goes quiet.
    assert_eq!(c.set_occluded(win(1), false), PacingAction::RequestRedraw);
    assert!(c.is_eligible(win(1)));
    assert_eq!(c.set_occluded(win(1), false), PacingAction::Sleep);
}

#[test]
fn set_visible_gates_presentation_like_occlusion() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    assert_eq!(c.set_visible(win(1), false), PacingAction::Sleep);
    assert!(!c.is_eligible(win(1)));
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert!(!plan.should_present);
    assert_eq!(c.set_visible(win(1), true), PacingAction::RequestRedraw);
}

#[test]
fn focus_change_does_not_gate_presentation() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    // Losing focus keeps the window eligible; no spurious recovery on regain.
    assert_eq!(c.set_focused(win(1), false), PacingAction::Sleep);
    assert!(c.is_eligible(win(1)));
    assert_eq!(c.set_focused(win(1), true), PacingAction::Sleep);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert!(plan.should_present);
}

#[test]
fn is_focused_reflects_set_focused_and_defaults_true() {
    let mut c = FrameCoordinator::new();
    // Unknown window defaults to focused (ambient effects not suppressed
    // before the first focus report).
    assert!(c.is_focused(win(1)));
    c.set_focused(win(1), false);
    assert!(!c.is_focused(win(1)));
    c.set_focused(win(1), true);
    assert!(c.is_focused(win(1)));
}

#[test]
fn removed_window_contributes_nothing() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(100))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    c.remove_window(win(1));
    assert_eq!(c.next_wake_deadline(), None);
    assert_eq!(c.active_reasons(win(1)), Vec::new());
}

#[test]
fn a_platform_redraw_presents_with_an_explicit_reason() {
    // A window-system RedrawRequested (expose, resize, first map) or a direct
    // request_redraw from device-lost recovery carries no coordinator demand.
    // The surface must still be repainted, so the plan presents -- and names
    // why, per architectural invariant 12 (every scheduled frame has at least
    // one inspectable demand reason).
    let mut c = FrameCoordinator::new();
    let now = t0();
    let planned = c.begin_frame(win(1), tick_at(now));
    assert!(!planned.should_present, "no demand explains this tick");
    assert!(planned.reasons.is_empty());

    let plan = c.platform_redraw_plan(win(1), tick_at(now));
    assert!(plan.should_present);
    assert_eq!(
        plan.work,
        RenderWork::RepaintLayers {
            layers: LayerMask::all(),
            damage: Damage::FullLayer
        },
        "nothing survives a surface invalidation, so the repaint is full"
    );
    assert_eq!(
        plan.reasons.iter().collect::<Vec<_>>(),
        vec![DemandReason::PlatformRedraw]
    );
}

#[test]
fn a_platform_redraw_presents_nothing_while_ineligible() {
    // An occluded surface shows nothing; a platform redraw for it is still not
    // a licence to present.
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.set_occluded(win(1), true);
    let plan = c.platform_redraw_plan(win(1), tick_at(now));
    assert!(!plan.should_present);
    assert_eq!(plan.work, RenderWork::None);
    assert!(plan.reasons.is_empty());
}

#[test]
fn a_planned_frame_with_real_demand_never_reaches_the_platform_redraw_path() {
    // Attribution must not upgrade work: a tick carrying a pending
    // composite-only blink plans that blink, so the caller never falls through
    // to the full repaint and the retained-static fast path keeps engaging.
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert!(plan.should_present);
    assert_eq!(
        plan.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS
        }
    );
    assert_eq!(
        plan.reasons.iter().collect::<Vec<_>>(),
        vec![DemandReason::CursorAnimation]
    );
}

#[test]
fn demand_reason_names_are_an_explicit_golden_in_index_order() {
    // The names are the diagnostics vocabulary (/metrics frame.demand_reasons),
    // so reordering or renaming a reason silently reinterprets recorded
    // captures. Spelled out rather than derived from the enum.
    assert_eq!(
        DemandReason::ALL
            .into_iter()
            .map(DemandReason::name)
            .collect::<Vec<_>>(),
        vec![
            "editor_commit",
            "cursor_animation",
            "cursor_color_cycle",
            "finite_effect",
            "transition",
            "video",
            "webkit",
            "shader_surface",
            "frame_shader",
            "terminal",
            "expose",
            "platform_redraw",
            "debug_capture",
            "redisplay",
            "cursor_effect",
            "window_effect",
            "text_effect",
            "scroll_effect",
            "decorative_effect",
            "transient_effect",
        ]
    );
    assert_eq!(DemandReason::COUNT, 20);
}

#[test]
fn demand_reason_indices_are_dense() {
    let mut seen = [false; DemandReason::COUNT];
    for reason in DemandReason::ALL {
        assert!(!seen[reason.index()], "duplicate index for {reason:?}");
        seen[reason.index()] = true;
    }
    assert!(
        seen.iter().all(|s| *s),
        "DemandReason::ALL must list every reason exactly once"
    );
}

#[test]
fn plan_attributes_the_frame_to_its_driving_reasons() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    c.submit_demand(win(1), editor_commit(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    assert!(plan.reasons.contains(DemandReason::CursorAnimation));
    assert!(plan.reasons.contains(DemandReason::EditorCommit));
    assert!(!plan.reasons.contains(DemandReason::WebKit));
    assert_eq!(
        plan.reasons.iter().collect::<Vec<_>>(),
        vec![DemandReason::EditorCommit, DemandReason::CursorAnimation]
    );
    // Consumed demand is not re-attributed to the next frame.
    let next = c.begin_frame(win(1), tick_at(now.plus(ms(2))));
    assert!(next.reasons.is_empty());
}

#[test]
fn an_ambient_24_hz_demand_attributes_every_frame_it_drives() {
    // The shape of the idle cursor-color-cycle load: one standing MaxRate
    // demand paces a frame per period, and each of those frames names it.
    let mut c = FrameCoordinator::new();
    let now = t0();
    let cycle = FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::MaxRate(std::num::NonZeroU16::new(24).unwrap()),
        reason: DemandReason::CursorColorCycle,
    };
    let mut attributed = 0;
    for i in 0..10 {
        let at = now.plus(ms(42 * i));
        c.submit_demand(win(1), cycle, at);
        let plan = c.begin_frame(win(1), tick_at(at));
        if plan.reasons.contains(DemandReason::CursorColorCycle) {
            attributed += 1;
        }
    }
    assert_eq!(attributed, 10);
}

#[test]
fn per_window_attribution_names_the_blinking_window_only() {
    // Design-doc Observability: "counters per native window and process-wide
    // totals ... active demand reasons". A window with a standing cursor-blink
    // deadline reports cursor_animation as an active per-window reason; an
    // idle sibling window reports none and accrues no per-window plan counts.
    use super::super::frame_stats;
    let mut c = FrameCoordinator::new();
    let now = t0();
    // Window 1: blink deadline (the shape declare_frame_demands submits).
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(500))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );
    // Window 2 exists (tracked by the coordinator) but has no demand.
    c.set_focused(win(2), true);

    frame_stats::publish_window_demand(c.window_demand());
    let snap = frame_stats::window_snapshots();
    let by_id = |id: u64| snap.iter().find(|w| w.window == id).unwrap();
    assert_eq!(by_id(1).active_reasons, vec!["cursor_animation"]);
    assert_eq!(by_id(2).active_reasons, Vec::<&str>::new());

    // The blink deadline fires; the planned frame is attributed per-window.
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(500))));
    assert!(plan.reasons.contains(DemandReason::CursorAnimation));
    frame_stats::count_plan(win(1), &plan);
    let snap = frame_stats::window_snapshots();
    let by_id = |id: u64| snap.iter().find(|w| w.window == id).unwrap();
    assert_eq!(
        by_id(1).demand_reasons[DemandReason::CursorAnimation.index()],
        1
    );
    assert_eq!(by_id(2).demand_reasons.iter().sum::<u64>(), 0);

    // Once the demand is consumed, republishing clears the active reason.
    frame_stats::publish_window_demand(c.window_demand());
    let snap = frame_stats::window_snapshots();
    assert!(snap.iter().all(|w| w.active_reasons.is_empty()));

    // A removed window's per-window stats do not outlive it.
    c.remove_window(win(2));
    frame_stats::publish_window_demand(c.window_demand());
    let snap = frame_stats::window_snapshots();
    assert!(snap.iter().all(|w| w.window != 2));
}

#[test]
fn an_elapsed_recovery_deadline_becomes_a_redraw_request() {
    // GNU never uses a ripe deadline as a select timeout: timer_check runs
    // every ripe timer and only then returns the wait (keyboard.c:4911-4945,
    // consumed as the pselect timeout at process.c:5490). Our loop's wake
    // deadline must obey the same rule, because a deadline nobody converts
    // into work is armed again on the next pass -- already elapsed -- and the
    // loop then spins at zero wait forever.
    //
    // A present that produced nothing schedules a bounded Expose retry, and
    // Expose has no producer in the demand reconciliation: servicing the
    // deadline is the only thing that can turn it into a frame.
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(win(1), composite_cursor(), now);
    let plan = c.begin_frame(win(1), tick_at(now.plus(ms(1))));
    c.finish_frame(win(1), &plan, PresentResult::Timeout, now.plus(ms(2)));

    let woke = now.plus(ms(60));
    // The state that produced the spin: an elapsed deadline sitting in the
    // schedule with nothing to convert it into a frame.
    assert!(
        c.next_wake_deadline_unserviced()
            .is_some_and(|at| at <= woke),
        "the retry deadline has come due"
    );
    let service = c.service_deadlines(woke);
    assert_eq!(
        service.redraw,
        vec![win(1)],
        "the ripe retry must be driven into one platform redraw request"
    );
    assert_eq!(
        service.wake,
        LoopWake::Idle,
        "nothing is left to wait for once the retry has been serviced"
    );

    // And the work survives to the frame that request delivers.
    let retry = c.begin_frame(win(1), tick_at(woke));
    assert_eq!(
        retry.work,
        RenderWork::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        }
    );
}

#[test]
fn servicing_never_leaves_an_elapsed_wake_deadline() {
    // The structural invariant the busy-spin violated: whatever the loop is
    // told to wait for is strictly in the future, so `WaitUntil` can never
    // degenerate into a zero wait. Asserted over every deadline-bearing
    // cadence, serviced repeatedly at times well past all of them.
    let mut c = FrameCoordinator::new();
    let now = t0();
    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::RepaintLayers {
                layers: LayerMask::all(),
                damage: Damage::FullLayer,
            },
            cadence: Cadence::At(now.plus(ms(10))),
            reason: DemandReason::Redisplay,
        },
        now,
    );
    c.submit_demand(
        win(2),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::MaxRate(std::num::NonZeroU16::new(60).unwrap()),
            reason: DemandReason::CursorColorCycle,
        },
        now,
    );
    let plan = c.begin_frame(win(2), tick_at(now));
    c.finish_frame(win(2), &plan, PresentResult::Timeout, now);

    for step in [500u64, 1_000, 5_000] {
        let woke = now.plus(ms(step));
        let service = c.service_deadlines(woke);
        if let LoopWake::At(deadline) = service.wake {
            assert!(
                deadline.event_time() > woke,
                "a serviced wake deadline must be strictly in the future"
            );
        }
    }
}

#[test]
fn a_ripe_at_demand_does_not_leave_its_old_deadline_behind() {
    // Submitting Cadence::At in the future records a deadline; submitting the
    // same reason again once that instant has passed fires it immediately.
    // The stale record must go with it -- otherwise it stays ripe forever in
    // the schedule and every pass re-arms an already-elapsed wake. This is the
    // same hazard the MaxRate expired-anchor branch clears explicitly.
    let mut c = FrameCoordinator::new();
    let now = t0();
    let at = now.plus(ms(10));
    let demand = FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::At(at),
        reason: DemandReason::CursorAnimation,
    };
    assert_eq!(
        c.submit_demand(win(1), demand, now),
        PacingAction::WakeAt(at)
    );
    assert_eq!(
        c.submit_demand(win(1), demand, at.plus(ms(1))),
        PacingAction::RequestRedraw
    );
    assert_eq!(
        c.next_wake_deadline(),
        None,
        "the deadline that just fired must not stay in the schedule"
    );
}

// =======================================================================
// One observation per about_to_wait pass
// =======================================================================
//
// The event loop used to read the clock four separate times in a single pass:
// once for stats, once for video, once for cursor ticks, once for scheduling.
// Servicing now happens at the pass's own timestamp, which is earlier than the
// separate read it replaced. These tests pin the consequence: a deadline that
// falls inside the collapsed window is deferred by exactly one pass and the
// loop is armed to wake for it — it is never dropped, and the loop never spins.

#[test]
fn a_deadline_just_after_the_service_time_defers_and_arms_the_wake() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let deadline = now.plus(ms(5));

    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(deadline),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );

    // Serviced before it is due: no redraw, but the loop is told to wake at
    // exactly the deadline rather than sleeping past it.
    let service = c.service_deadlines(now.plus(ms(4)));
    assert!(service.redraw.is_empty(), "not ripe yet");
    assert_eq!(service.wake, LoopWake::At(FutureDeadline(deadline)));
}

#[test]
fn the_deferred_deadline_is_serviced_on_the_next_pass() {
    let mut c = FrameCoordinator::new();
    let now = t0();
    let deadline = now.plus(ms(5));

    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(deadline),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );

    let early = c.service_deadlines(now.plus(ms(4)));
    assert!(early.redraw.is_empty());

    let ripe = c.service_deadlines(deadline);
    assert_eq!(ripe.redraw, vec![win(1)], "deferred, never dropped");
    assert_eq!(ripe.wake, LoopWake::Idle, "and not rearmed once consumed");
}

#[test]
fn servicing_twice_at_one_timestamp_is_idempotent() {
    // The pass services, runs the video producer, then services again at the
    // *same* timestamp. The second call must find nothing ripe, which is what
    // lets the debug_assert in that path hold.
    let mut c = FrameCoordinator::new();
    let now = t0();

    c.submit_demand(
        win(1),
        FrameDemand {
            invalidation: Invalidation::CompositeOnly {
                layers: LayerMask::CURSOR_EFFECTS,
            },
            cadence: Cadence::At(now.plus(ms(5))),
            reason: DemandReason::CursorAnimation,
        },
        now,
    );

    let first = c.service_deadlines(now.plus(ms(5)));
    assert_eq!(first.redraw, vec![win(1)]);
    let second = c.service_deadlines(now.plus(ms(5)));
    assert!(
        second.redraw.is_empty(),
        "nothing ripe survives one service"
    );
}
