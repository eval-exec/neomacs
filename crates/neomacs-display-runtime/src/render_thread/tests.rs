use super::RenderApp;
use super::render_quality::RenderBackendProfile;
use super::state::GuiChromeInteractionState;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::core::types::DisplayWindowId;
use crate::thread_comm::{
    ClipboardCommand, ClipboardSelection, RenderCommand, ThreadComms, UiCommand, WindowCommand,
};
use crate::thread_comm::{FrameRef, FrameShaderAvailability};
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_protocol::{
    Color, CursorStyle, DisplaySlotId, EffectsConfig, FrameRate, PhysCursor, PopupMenuItem,
    SealedFramePresentation,
};
use neovm_core::window::GuiFrameGeometryHints;
use std::sync::{Arc, Mutex};
use winit::keyboard::{Key, NamedKey};

fn make_test_app() -> RenderApp {
    let comms = ThreadComms::new();
    let (_emacs, render) = comms.split();
    RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    )
}

#[test]
fn child_frames_start_with_square_corners() {
    let app = make_test_app();

    assert_eq!(app.child_frame_style.corner_radius, 0.0);
}

#[test]
fn entering_cpu_compatibility_mode_discards_retained_effect_demand() {
    let mut app = make_test_app();
    let renderer_effects = &mut app
        .frame_windows
        .primary_window_mut()
        .expect("test app has a primary window")
        .render
        .compositor
        .renderer_effects;
    renderer_effects.spawn_ripple(10.0, 20.0);
    assert!(renderer_effects.needs_redraw());

    app.install_backend_profile(RenderBackendProfile::software());

    assert!(
        !app.frame_windows
            .primary_window()
            .expect("test app has a primary window")
            .render
            .compositor
            .renderer_effects
            .needs_redraw(),
        "disabled effects must not retain max-rate scheduler demand"
    );
}

#[test]
fn backend_recovery_rederives_quality_from_the_unchanged_request() {
    let mut app = make_test_app();
    app.requested_visual_config.cursor_motion.enabled = true;
    let requested = app.requested_visual_config.clone();

    app.install_backend_profile(RenderBackendProfile::hardware());
    assert_eq!(app.render_policy.effective_visual_config(), &requested);
    assert_eq!(
        app.comms.capabilities.frame_shader_availability(),
        FrameShaderAvailability::Available
    );

    app.install_backend_profile(RenderBackendProfile::software());
    assert!(app.requested_visual_config.cursor_motion.enabled);
    assert!(
        !app.render_policy
            .effective_visual_config()
            .cursor_motion
            .enabled
    );
    assert_eq!(
        app.comms.capabilities.frame_shader_availability(),
        FrameShaderAvailability::SuppressedByQualityPolicy
    );

    app.install_backend_profile(RenderBackendProfile::hardware());
    assert_eq!(app.render_policy.effective_visual_config(), &requested);
    assert_eq!(
        app.comms.capabilities.frame_shader_availability(),
        FrameShaderAvailability::Available
    );
}

#[test]
fn suppressed_direct_frame_shader_command_reports_failure_to_evaluator() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_owned(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    app.install_backend_profile(RenderBackendProfile::software());
    let prepared = app.comms.capabilities.prepare_frame_shader_request(true);
    let request = prepared.id();
    prepared.commit();

    app.handle_asset(crate::thread_comm::AssetCommand::FrameShaderSet {
        request,
        composed: Some((
            "unused".to_owned(),
            crate::shader_surface::SurfaceShaderLanguage::Wgsl,
            Vec::new(),
        )),
    });

    match emacs.input_rx.recv().expect("failure event") {
        crate::thread_comm::InputEvent::FrameShaderFailed { error } => {
            assert!(error.contains("render-quality policy"), "{error}");
        }
        other => panic!("expected frame-shader failure, got {other:?}"),
    }

    app.handle_asset(crate::thread_comm::AssetCommand::FrameShaderSetUniform {
        request,
        name: "gain".to_owned(),
        value: [0.5, 0.0, 0.0, 0.0],
    });
    match emacs.input_rx.recv().expect("uniform failure event") {
        crate::thread_comm::InputEvent::FrameShaderFailed { error } => {
            assert!(error.contains("uniform updates"), "{error}");
        }
        other => panic!("expected frame-shader uniform failure, got {other:?}"),
    }
}

#[test]
fn removing_frame_shader_leaves_one_unshaded_repaint_demand() {
    let mut app = make_test_app();
    let primary = app
        .frame_windows
        .primary_window_mut()
        .expect("test app has a primary window");
    primary.render.set_dirty(false);
    assert!(!primary.render.compositor.dirty);
    let prepared = app.comms.capabilities.prepare_frame_shader_request(false);
    let request = prepared.id();
    prepared.commit();

    app.handle_asset(crate::thread_comm::AssetCommand::FrameShaderSet {
        request,
        composed: None,
    });

    assert!(
        app.frame_windows
            .primary_window()
            .expect("test app has a primary window")
            .render
            .compositor
            .dirty,
        "shader removal must repaint once after retracting its continuous demand"
    );
}

#[test]
fn cursor_color_cycle_rate_respects_effect_and_display_limits() {
    let display_60_hz = std::num::NonZeroU16::new(60).unwrap();

    assert_eq!(
        RenderApp::cursor_color_cycle_rate(FrameRate::new(24).unwrap(), display_60_hz),
        std::num::NonZeroU16::new(24).unwrap()
    );
    assert_eq!(
        RenderApp::cursor_color_cycle_rate(FrameRate::new(144).unwrap(), display_60_hz),
        display_60_hz
    );
}

#[test]
fn reported_display_rate_is_a_hard_cap_even_below_30_hz() {
    assert_eq!(
        RenderApp::display_rate_limit(Some(24_000)),
        std::num::NonZeroU16::new(24).unwrap()
    );
    assert_eq!(
        RenderApp::display_rate_limit(Some(23_976)),
        std::num::NonZeroU16::new(23).unwrap(),
        "the integer scheduler must conservatively floor a fractional display rate"
    );
    assert_eq!(
        RenderApp::cursor_color_cycle_rate(
            FrameRate::new(60).unwrap(),
            RenderApp::display_rate_limit(Some(24_000)),
        ),
        std::num::NonZeroU16::new(24).unwrap()
    );
}

fn frame_with_cursor_effects(
    effects: Option<EffectsConfig>,
    cursor_style: CursorStyle,
) -> FrameGlyphBuffer {
    let window_id = DisplayWindowId::new(7);
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.set_phys_cursor(PhysCursor {
        window_id,
        charpos: 0,
        row: 0,
        col: 0,
        slot_id: DisplaySlotId::ZERO,
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: cursor_style,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });
    if let Some(effects) = effects {
        frame.set_window_cursor_effects(window_id, effects);
    }
    frame
}

#[test]
fn cursor_color_cycle_cadence_follows_the_effective_frame_profile() {
    let display_60_hz = std::num::NonZeroU16::new(60).unwrap();
    let mut global = EffectsConfig::default();
    global.cursor_color_cycle.enabled = false;
    let frame = frame_with_cursor_effects(None, CursorStyle::FilledBox);
    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&frame, &global, display_60_hz, true),
        None
    );

    let mut local = EffectsConfig::cursor_profile_baseline();
    local.cursor_color_cycle.enabled = true;
    local.cursor_color_cycle.fps = FrameRate::new(12).unwrap();
    let frame = frame_with_cursor_effects(Some(local), CursorStyle::FilledBox);
    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&frame, &global, display_60_hz, true),
        std::num::NonZeroU16::new(12)
    );

    global.cursor_color_cycle.enabled = true;
    let frame = frame_with_cursor_effects(
        Some(EffectsConfig::cursor_profile_baseline()),
        CursorStyle::FilledBox,
    );
    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&frame, &global, display_60_hz, true),
        None
    );
}

#[test]
fn cursor_color_cycle_cadence_pauses_when_the_cycle_cannot_change_pixels() {
    let display_60_hz = std::num::NonZeroU16::new(60).unwrap();
    let global = EffectsConfig::default();
    let filled = frame_with_cursor_effects(None, CursorStyle::FilledBox);
    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&filled, &global, display_60_hz, false),
        None,
        "a blinked-off filled cursor has no visible cycle work"
    );

    let hollow = frame_with_cursor_effects(None, CursorStyle::Hollow);
    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&hollow, &global, display_60_hz, true),
        None,
        "the renderer deliberately does not color-cycle hollow cursors"
    );
}

#[test]
fn cursor_color_cycle_cadence_covers_every_rendered_window_cursor() {
    let display_60_hz = std::num::NonZeroU16::new(60).unwrap();
    let global = EffectsConfig::cursor_profile_baseline();

    let mut selected = EffectsConfig::cursor_profile_baseline();
    selected.cursor_color_cycle.enabled = true;
    selected.cursor_color_cycle.fps = FrameRate::new(12).unwrap();
    let mut frame = frame_with_cursor_effects(Some(selected), CursorStyle::FilledBox);

    let decorative_window = DisplayWindowId::new(8);
    frame.add_cursor(
        decorative_window,
        80.0,
        0.0,
        2.0,
        16.0,
        CursorStyle::Bar(2.0),
        Color::WHITE,
    );
    let mut decorative = EffectsConfig::cursor_profile_baseline();
    decorative.cursor_color_cycle.enabled = true;
    decorative.cursor_color_cycle.fps = FrameRate::new(24).unwrap();
    frame.set_window_cursor_effects(decorative_window, decorative);

    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&frame, &global, display_60_hz, true),
        std::num::NonZeroU16::new(24),
        "demand must use the fastest visible cursor rendered in a split frame"
    );

    frame.active_cursor_mut().unwrap().style = CursorStyle::Hollow;
    assert_eq!(
        RenderApp::cursor_color_cycle_cadence(&frame, &global, display_60_hz, true),
        std::num::NonZeroU16::new(24),
        "a hollow selected cursor must not suppress a cycling decorative cursor"
    );
}

#[test]
fn cursor_color_cycle_state_translates_to_an_exact_scheduler_demand() {
    use super::frame_sched::{Cadence, DemandReason, FrameDemand, Invalidation, LayerMask};

    let display_60_hz = std::num::NonZeroU16::new(60).unwrap();
    let global = EffectsConfig::default();
    let frame = frame_with_cursor_effects(None, CursorStyle::FilledBox);
    let expected = FrameDemand {
        invalidation: Invalidation::CompositeOnly {
            layers: LayerMask::CURSOR_EFFECTS,
        },
        cadence: Cadence::MaxRate(std::num::NonZeroU16::new(24).unwrap()),
        reason: DemandReason::CursorColorCycle,
    };

    assert_eq!(
        RenderApp::cursor_color_cycle_demand(&frame, &global, display_60_hz, true, true),
        Some(expected)
    );
    assert_eq!(
        RenderApp::cursor_color_cycle_demand(&frame, &global, display_60_hz, false, true),
        None,
        "blinked-off cursor state must retract standing demand"
    );
    assert_eq!(
        RenderApp::cursor_color_cycle_demand(&frame, &global, display_60_hz, true, false),
        None,
        "unfocused windows must retract standing demand"
    );
}

#[test]
fn cursor_color_cycle_reconciliation_drives_attributed_frames_and_retracts() {
    use super::frame_sched::{
        ClockSource, DemandReason, FrameCoordinator, FrameTick, NativeWindowId, PacingAction,
        RenderWork,
    };

    let mut coordinator = FrameCoordinator::new();
    let id = NativeWindowId(7);
    let now = neomacs_display_protocol::frame_time::observe_platform_now();
    let display_60_hz = std::num::NonZeroU16::new(60).unwrap();
    let global = EffectsConfig::default();
    let frame = frame_with_cursor_effects(None, CursorStyle::FilledBox);
    let tick = |at| FrameTick {
        frame_time: at,
        target_presentation_time: at,
        estimated_interval: std::time::Duration::from_secs_f64(1.0 / 60.0),
        source: ClockSource::Synthetic,
    };

    assert_eq!(
        RenderApp::reconcile_cursor_color_cycle_demand(
            &mut coordinator,
            id,
            Some(&frame),
            &global,
            display_60_hz,
            true,
            now,
        ),
        PacingAction::RequestRedraw
    );
    let plan = coordinator.begin_frame(id, tick(now));
    assert_eq!(
        plan.work,
        RenderWork::CompositeOnly {
            layers: super::frame_sched::LayerMask::CURSOR_EFFECTS,
        }
    );
    assert!(plan.reasons.contains(DemandReason::CursorColorCycle));
    assert!(!plan.reasons.contains(DemandReason::PlatformRedraw));

    assert!(matches!(
        RenderApp::reconcile_cursor_color_cycle_demand(
            &mut coordinator,
            id,
            Some(&frame),
            &global,
            display_60_hz,
            true,
            now.plus(std::time::Duration::from_millis(1)),
        ),
        PacingAction::WakeAt(_)
    ));
    assert_eq!(
        RenderApp::reconcile_cursor_color_cycle_demand(
            &mut coordinator,
            id,
            Some(&frame),
            &global,
            display_60_hz,
            false,
            now.plus(std::time::Duration::from_millis(2)),
        ),
        PacingAction::Sleep,
        "a blinked-off cursor must withdraw its standing deadline"
    );
    assert!(coordinator.active_reasons(id).is_empty());
    assert_eq!(coordinator.next_wake_deadline_unserviced(), None);

    assert_eq!(
        RenderApp::reconcile_cursor_color_cycle_demand(
            &mut coordinator,
            id,
            Some(&frame),
            &global,
            display_60_hz,
            true,
            now.plus(std::time::Duration::from_millis(3)),
        ),
        PacingAction::RequestRedraw
    );
    let _ = coordinator.begin_frame(id, tick(now.plus(std::time::Duration::from_millis(3))));
    assert!(matches!(
        RenderApp::reconcile_cursor_color_cycle_demand(
            &mut coordinator,
            id,
            Some(&frame),
            &global,
            display_60_hz,
            true,
            now.plus(std::time::Duration::from_millis(4)),
        ),
        PacingAction::WakeAt(_)
    ));
    coordinator.set_focused(id, false);
    assert_eq!(
        RenderApp::reconcile_cursor_color_cycle_demand(
            &mut coordinator,
            id,
            Some(&frame),
            &global,
            display_60_hz,
            true,
            now.plus(std::time::Duration::from_millis(5)),
        ),
        PacingAction::Sleep,
        "an unfocused window must withdraw its standing deadline"
    );
    assert!(coordinator.active_reasons(id).is_empty());
    assert_eq!(coordinator.next_wake_deadline_unserviced(), None);
}

fn seal_state(mut state: FrameDisplayState) -> SealedFramePresentation {
    if state.presentation_id == neomacs_display_protocol::PresentationId::default() {
        state.presentation_id = neomacs_display_protocol::PresentationId::new(1);
        let placement = state.frame_placement;
        state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
            placement.frame(),
            state.presentation_id,
            placement.parent(),
            placement.outer_in_parent(),
            placement.z_order(),
        );
    }
    state.presented_hit_index = neomacs_display_protocol::PresentedHitIndex::from_parts(
        state.presentation_id,
        vec![],
        vec![],
    )
    .unwrap();
    SealedFramePresentation::seal(state).unwrap()
}

fn presentation_state(frame_id: u64, parent_id: u64, presentation: u64) -> SealedFramePresentation {
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = neomacs_display_protocol::PresentationId::new(presentation);
    frame.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(frame_id),
        neomacs_display_protocol::DisplayFrameId::new(parent_id),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    seal_state(FrameDisplayState::from_frame_glyph_buffer(&frame))
}

fn make_test_device() -> Option<wgpu::Device> {
    let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = wgpu::Backends::all();
    let instance = wgpu::Instance::new(instance_descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    let (device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("render-thread test device"),
        ..Default::default()
    }))
    .ok()?;
    Some(device)
}

#[test]
fn test_translate_key_named() {
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Escape)),
        0xff1b
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Enter)),
        0xff0d
    );
    assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Tab)), 0xff09);
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Backspace)),
        0xff08
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Delete)),
        0xffff
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Home)),
        0xff50
    );
    assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::End)), 0xff57);
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::PageUp)),
        0xff55
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::PageDown)),
        0xff56
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowLeft)),
        0xff51
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowUp)),
        0xff52
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowRight)),
        0xff53
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowDown)),
        0xff54
    );
    assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Space)), 0x20);
}

#[test]
fn test_translate_key_character() {
    assert_eq!(
        RenderApp::translate_key(&Key::Character("a".into())),
        'a' as u32
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Character("A".into())),
        'A' as u32
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Character("1".into())),
        '1' as u32
    );
}

#[test]
fn test_translate_key_function_keys() {
    assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::F1)), 0xffbe);
    assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::F12)), 0xffc9);
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Insert)),
        0xff63
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::PrintScreen)),
        0xff61
    );
}

#[test]
fn test_translate_key_unknown() {
    assert_eq!(RenderApp::translate_key(&Key::Dead(None)), 0);
}

#[test]
fn test_render_thread_creation() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();

    assert!(emacs.input_rx.is_empty());
    assert!(render.cmd_rx.is_empty());
}

#[test]
fn clipboard_command_before_display_initialization_returns_an_explicit_error() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
    emacs
        .cmd_tx
        .send(RenderCommand::Clipboard(ClipboardCommand::GetText {
            selection: ClipboardSelection::Clipboard,
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(5),
            reply: reply_tx,
        }))
        .unwrap();

    assert!(!app.process_commands());
    assert_eq!(
        reply_rx.recv().unwrap(),
        Err("clipboard is unavailable before display initialization".to_owned())
    );

    let (owner_reply_tx, owner_reply_rx) = crossbeam_channel::bounded(1);
    emacs
        .cmd_tx
        .send(RenderCommand::Clipboard(ClipboardCommand::GetOwnership {
            selection: ClipboardSelection::Primary,
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(5),
            reply: owner_reply_tx,
        }))
        .unwrap();

    assert!(!app.process_commands());
    assert_eq!(
        owner_reply_rx.recv().unwrap(),
        Err("clipboard is unavailable before display initialization".to_owned())
    );
}

#[test]
fn destroy_primary_window_command_prevents_lifecycle_recreate() {
    let mut app = make_test_app();
    app.frame_windows.adopt_primary_frame_id(0x1000);

    app.handle_window(WindowCommand::DestroyWindow {
        frame: FrameRef::Primary,
    });

    assert!(app.frame_windows.primary_window().is_none());
    assert!(
        app.frame_windows
            .primary_window()
            .map(|ws| &ws.render)
            .is_none()
    );
    assert!(
        app.frame_windows
            .primary_window()
            .and_then(|ws| ws.render.compositor.current_frame.as_ref())
            .is_none()
    );
    assert!(
        !app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
    assert!(app.frame_windows.primary_window().is_none());
    assert_eq!(app.frame_windows.primary_frame_id(), None);
}

#[test]
fn destroy_adopted_primary_window_by_real_frame_id_prevents_lifecycle_recreate() {
    let mut app = make_test_app();
    app.frame_windows.adopt_primary_frame_id(0x1000);

    app.handle_window(WindowCommand::DestroyWindow {
        frame: FrameRef::Frame(0x1000),
    });

    assert!(app.frame_windows.primary_window().is_none());
    assert!(
        app.frame_windows
            .primary_window()
            .map(|ws| &ws.render)
            .is_none()
    );
    assert!(
        !app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
    assert!(app.frame_windows.primary_window().is_none());
    assert_eq!(app.frame_windows.primary_frame_id(), None);
    assert!(app.frame_windows.pending_destroys.is_empty());
}

#[test]
fn pending_dirty_primary_window_is_not_redrawable_active_work() {
    let mut app = make_test_app();
    let primary = app.frame_windows.primary_window_mut().unwrap();
    primary.render.compositor.dirty = true;

    assert!(
        app.frame_windows
            .primary_window()
            .unwrap()
            .render
            .compositor
            .dirty
    );
    assert!(
        !app.frame_windows
            .windows
            .values()
            .any(|window_state| window_state.has_presentable_dirty_content()),
        "a pending window has no native surface to receive RedrawRequested"
    );
}

#[test]
fn pre_bootstrap_primary_resize_updates_pending_size() {
    let mut app = make_test_app();
    let geometry_hints = GuiFrameGeometryHints {
        base_width: 24,
        base_height: 32,
        min_width: 48,
        min_height: 64,
        width_inc: 8,
        height_inc: 16,
    };

    app.handle_window(WindowCommand::ResizeWindow {
        frame: FrameRef::Primary,
        width: 1024,
        height: 768,
        geometry_hints,
    });

    assert_eq!(
        app.frame_windows
            .primary_window()
            .map_or((0, 0), |ws| ws.native_size()),
        (1024, 768)
    );
    let primary = app.frame_windows.primary_window().unwrap();
    assert_eq!(primary.lifecycle.geometry_hints(), Some(geometry_hints));
}

#[test]
fn pre_bootstrap_set_window_size_updates_native_fallback_size() {
    let mut app = make_test_app();

    app.handle_window(WindowCommand::SetWindowSize {
        width: 900,
        height: 700,
    });

    assert_eq!(
        app.frame_windows
            .primary_window()
            .map_or((0, 0), |ws| ws.native_size()),
        (900, 700)
    );
}

#[test]
fn pre_bootstrap_window_decorations_update_native_fallback_chrome() {
    let mut app = make_test_app();

    app.handle_window(WindowCommand::SetWindowDecorated { decorated: false });

    assert!(
        !app.frame_windows
            .primary_window()
            .expect("primary window state")
            .chrome()
            .decorations_enabled
    );
}

#[test]
fn adopt_primary_window_command_updates_existing_primary_render_state_identity() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }

    app.handle_window(WindowCommand::AdoptPrimaryFrame {
        frame: FrameRef::Frame(0x1000),
    });

    assert_eq!(app.frame_windows.primary_frame_id(), Some(0x1000));
    assert_eq!(
        app.frame_windows
            .primary_window()
            .map(|ws| &ws.render)
            .map(|frame| frame.emacs_frame_id),
        Some(0x1000)
    );
}

#[test]
fn adopted_primary_frame_id_targets_primary_popup_menu() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }
    app.frame_windows.adopt_primary_frame_id(0x1000);

    app.handle_ui(UiCommand::ShowPopupMenu {
        frame: FrameRef::Frame(0x1000),
        placement: neomacs_display_protocol::PopupPlacement::at(
            neomacs_display_protocol::Point::new(10.0, 20.0),
        ),
        items: vec![PopupMenuItem {
            label: "Open".to_string(),
            shortcut: String::new(),
            enabled: true,
            separator: false,
            submenu: false,
            depth: 0,
        }],
        title: None,
        fg: None,
        bg: None,
    });

    assert!(
        app.frame_windows
            .primary_window()
            .and_then(|ws| ws.render.overlays.popup_menu.as_ref())
            .is_some()
    );
    assert!(
        app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
}

#[test]
fn primary_tooltip_command_marks_render_state_dirty() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }

    app.handle_ui(UiCommand::ShowTooltip {
        frame: FrameRef::Primary,
        x: 10.0,
        y: 20.0,
        text: "tip".to_string(),
        fg_r: 1.0,
        fg_g: 1.0,
        fg_b: 1.0,
        bg_r: 0.0,
        bg_g: 0.0,
        bg_b: 0.0,
    });

    assert!(
        app.frame_windows
            .primary_window()
            .map(|ws| &ws.render)
            .and_then(|frame| frame.overlays.tooltip.as_ref())
            .is_some()
    );
    assert!(
        app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
}

#[test]
fn hide_popup_menu_marks_primary_chrome_dirty_without_popup() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }
    if let Some(ws) = app.frame_windows.primary_window_mut() {
        ws.render
            .with_chrome_interaction_mut(|chrome| chrome.menu_bar_active = Some(3))
    } else {
        false
    };
    if let Some(ws) = app.frame_windows.primary_window_mut() {
        ws.render.compositor.dirty = false
    };

    app.handle_ui(UiCommand::HidePopupMenu);

    assert_eq!(
        app.frame_windows
            .primary_window()
            .map_or(GuiChromeInteractionState::default(), |ws| ws
                .render
                .chrome
                .interaction)
            .menu_bar_active,
        None
    );
    assert!(
        app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
}

#[test]
fn popup_menu_for_unknown_secondary_does_not_fall_back_to_primary() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }

    app.handle_ui(UiCommand::ShowPopupMenu {
        frame: FrameRef::Frame(0x2000),
        placement: neomacs_display_protocol::PopupPlacement::at(
            neomacs_display_protocol::Point::new(10.0, 20.0),
        ),
        items: vec![PopupMenuItem {
            label: "Open".to_string(),
            shortcut: String::new(),
            enabled: true,
            separator: false,
            submenu: false,
            depth: 0,
        }],
        title: None,
        fg: None,
        bg: None,
    });

    assert!(
        app.frame_windows
            .primary_window()
            .and_then(|ws| ws.render.overlays.popup_menu.as_ref())
            .is_none()
    );
    assert!(
        !app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
}

#[test]
fn tooltip_for_unknown_secondary_does_not_fall_back_to_primary() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }

    app.handle_ui(UiCommand::ShowTooltip {
        frame: FrameRef::Frame(0x2000),
        x: 10.0,
        y: 20.0,
        text: "secondary".to_string(),
        fg_r: 1.0,
        fg_g: 1.0,
        fg_b: 1.0,
        bg_r: 0.0,
        bg_g: 0.0,
        bg_b: 0.0,
    });

    assert!(
        app.frame_windows
            .primary_window()
            .map(|ws| &ws.render)
            .and_then(|frame| frame.overlays.tooltip.as_ref())
            .is_none()
    );
    assert!(
        !app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
}

#[test]
fn adopted_primary_frame_id_targets_primary_visual_bell() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }
    app.frame_windows.adopt_primary_frame_id(0x1000);

    app.handle_ui(UiCommand::VisualBell {
        frame: FrameRef::Frame(0x1000),
    });

    assert!(
        app.frame_windows
            .primary_window()
            .map(|ws| &ws.render)
            .and_then(|frame| frame.overlays.visual_bell_start)
            .is_some()
    );
    assert!(
        app.frame_windows
            .primary_window()
            .is_some_and(|ws| ws.render.compositor.dirty)
    );
}

#[test]
fn managed_primary_visual_bell_uses_frame_renderer_effects() {
    let mut render = make_test_device().map(|device| {
        super::frame_windows::GuiFrameRenderState::new(
            0x1000,
            &device,
            1.0,
            false,
            neomacs_display_protocol::frame_time::observe_platform_now(),
        )
    });
    let Some(render) = render.as_mut() else {
        return;
    };
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.add_window_info(
        DisplayWindowId::new(7),
        1,
        1,
        50,
        50,
        neomacs_display_protocol::presentation_origin::BufferModiff::default(),
        0.0,
        0.0,
        400.0,
        300.0,
        20.0,
        0.0,
        0.0,
        true,
        false,
        17.0,
        String::new(),
        String::new(),
        false,
    );
    render.compositor.current_frame = Some(frame);

    render.trigger_visual_bell(
        true,
        true,
        120,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );

    assert!(render.overlays.visual_bell_start.is_some());
    assert!(render.compositor.renderer_effects.has_transient_effects());
    assert!(render.compositor.dirty);
}

#[test]
fn adopted_primary_pointer_target_uses_real_frame_id() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }
    app.frame_windows.adopt_primary_frame_id(0x1000);
    if let Some(ws) = app.frame_windows.primary_window_mut() {
        ws.render.set_current_frame(
            Some(FrameGlyphBuffer::with_size(800.0, 600.0)),
            None,
            Default::default(),
        );
    };

    let (x, y, frame_id) = app.pointer_target_at(12.0, 34.0);

    assert_eq!((x, y), (12.0, 34.0));
    assert_eq!(frame_id, 0x1000);
}

#[test]
fn unknown_secondary_frame_snapshot_does_not_fall_back_to_primary() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }
    if let Some(ws) = app.frame_windows.primary_window_mut() {
        ws.render.set_current_frame(
            Some(FrameGlyphBuffer::with_size(800.0, 600.0)),
            None,
            Default::default(),
        );
    };
    if let Some(ws) = app.frame_windows.primary_window_mut() {
        ws.render.compositor.dirty = false
    };

    let mut secondary = FrameGlyphBuffer::with_size(320.0, 240.0);
    secondary.set_frame_identity(
        neomacs_display_protocol::types::DisplayFrameId::new(0x2000),
        neomacs_display_protocol::types::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::types::Color::BLACK,
        false,
        1.0,
    );
    emacs
        .frame_tx
        .send(seal_state(FrameDisplayState::from_frame_glyph_buffer(
            &secondary,
        )))
        .expect("queue secondary snapshot");

    app.poll_frame();

    assert_eq!(
        app.frame_windows
            .primary_window()
            .and_then(|ws| ws.render.compositor.current_frame.as_ref())
            .map(|frame| frame.width),
        Some(800.0)
    );
}

#[test]
fn installing_frame_emits_activation_before_replaced_presentation_retirement() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    app.frame_windows.adopt_primary_frame_id(0x42);

    emacs
        .frame_tx
        .send(presentation_state(0x42, 0, 41))
        .expect("queue initial presentation");
    app.poll_frame();
    let events = emacs.input_rx.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        events.as_slice(),
        [crate::thread_comm::InputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x42,
        }]
    ));

    emacs
        .frame_tx
        .send(presentation_state(0x42, 0, 42))
        .expect("queue replacement presentation");
    app.poll_frame();
    let events = emacs.input_rx.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        events.as_slice(),
        [
            crate::thread_comm::InputEvent::PresentationActivated {
                presentation: 42,
                emacs_frame_id: 0x42,
            },
            crate::thread_comm::InputEvent::PresentationRetired { presentation: 41 },
        ]
    ));
}

#[test]
fn superseded_pending_frame_is_discarded_before_activation() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );

    emacs
        .frame_tx
        .send(presentation_state(0x51, 0x50, 51))
        .expect("queue first deferred child");
    emacs
        .frame_tx
        .send(presentation_state(0x51, 0x50, 52))
        .expect("queue replacement deferred child");
    app.poll_frame();

    assert_eq!(app.pending_child_frames.len(), 1);
    let events = emacs.input_rx.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        events.as_slice(),
        [crate::thread_comm::InputEvent::PresentationDiscarded {
            presentation: 51,
            emacs_frame_id: 0x51,
        }]
    ));
}

#[test]
fn poll_frame_routes_nested_child_through_its_presented_ancestor_to_the_root_window() {
    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let mut app = RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    app.frame_windows.adopt_primary_frame_id(0x42);

    let mut root = FrameGlyphBuffer::with_size(800.0, 600.0);
    root.presentation_id = neomacs_display_protocol::PresentationId::new(40);
    root.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(0x42),
        neomacs_display_protocol::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    let mut parent = FrameGlyphBuffer::with_size(300.0, 200.0);
    parent.presentation_id = neomacs_display_protocol::PresentationId::new(41);
    parent.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(0x50),
        neomacs_display_protocol::DisplayFrameId::new(0x42),
        100.0,
        80.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    let mut nested = FrameGlyphBuffer::with_size(120.0, 80.0);
    nested.presentation_id = neomacs_display_protocol::PresentationId::new(42);
    nested.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(0x51),
        neomacs_display_protocol::DisplayFrameId::new(0x50),
        15.0,
        12.0,
        2,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    emacs
        .frame_tx
        .send(seal_state(FrameDisplayState::from_frame_glyph_buffer(
            &nested,
        )))
        .unwrap();
    app.poll_frame();
    assert_eq!(app.pending_child_frames.len(), 1);
    assert!(
        app.frame_windows
            .primary_window()
            .unwrap()
            .render
            .compositor
            .child_frames
            .frames
            .is_empty()
    );
    assert!(emacs.input_rx.is_empty());

    emacs
        .frame_tx
        .send(seal_state(FrameDisplayState::from_frame_glyph_buffer(
            &parent,
        )))
        .unwrap();
    app.poll_frame();
    assert_eq!(app.pending_child_frames.len(), 2);
    assert!(emacs.input_rx.is_empty());

    emacs
        .frame_tx
        .send(seal_state(FrameDisplayState::from_frame_glyph_buffer(
            &root,
        )))
        .unwrap();
    app.poll_frame();

    {
        let window = app.frame_windows.primary_window().unwrap();
        let nested = window
            .render
            .compositor
            .child_frames
            .frames
            .get(&0x51)
            .expect("nested child routed to root window");
        assert_eq!((nested.abs_x, nested.abs_y), (115.0, 92.0));
        assert_eq!(window.render.compositor.child_frames.frames.len(), 2);
    }
    assert!(app.pending_child_frames.is_empty());
    let events = emacs.input_rx.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        events.as_slice(),
        [
            crate::thread_comm::InputEvent::PresentationActivated {
                presentation: 40,
                emacs_frame_id: 0x42,
            },
            crate::thread_comm::InputEvent::PresentationActivated {
                presentation: 41,
                emacs_frame_id: 0x50,
            },
            crate::thread_comm::InputEvent::PresentationActivated {
                presentation: 42,
                emacs_frame_id: 0x51,
            },
        ]
    ));

    let mut cyclic_parent = FrameGlyphBuffer::with_size(300.0, 200.0);
    cyclic_parent.presentation_id = neomacs_display_protocol::PresentationId::new(43);
    cyclic_parent.set_frame_identity(
        neomacs_display_protocol::DisplayFrameId::new(0x50),
        neomacs_display_protocol::DisplayFrameId::new(0x51),
        100.0,
        80.0,
        1,
        false,
        0.0,
        neomacs_display_protocol::Color::BLACK,
        false,
        1.0,
    );
    emacs
        .frame_tx
        .send(seal_state(FrameDisplayState::from_frame_glyph_buffer(
            &cyclic_parent,
        )))
        .unwrap();
    app.poll_frame();
    assert_eq!(
        app.frame_windows
            .primary_window()
            .unwrap()
            .render
            .compositor
            .child_frames
            .frames[&0x50]
            .frame
            .presentation_id
            .get(),
        41,
        "invalid cycle must preserve the previously coherent scene"
    );
    let events = emacs.input_rx.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        events.as_slice(),
        [crate::thread_comm::InputEvent::PresentationDiscarded {
            presentation: 43,
            emacs_frame_id: 0x50,
        }]
    ));

    app.handle_window(WindowCommand::RemoveChildFrame { frame_id: 0x50 });
    let mut retired = emacs
        .input_rx
        .try_iter()
        .filter_map(|event| match event {
            crate::thread_comm::InputEvent::PresentationRetired { presentation } => {
                Some(presentation)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    retired.sort_unstable();
    assert_eq!(retired, vec![41, 42]);
    assert!(
        app.frame_windows
            .primary_window()
            .unwrap()
            .render
            .compositor
            .child_frames
            .frames
            .is_empty()
    );
}
