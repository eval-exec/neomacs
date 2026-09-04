use super::RenderApp;
use super::frame_windows::GuiFrameRenderState;
use super::state::{effective_window_scale_factor, emacs_pixels_from_window_size};
use crate::backend::wgpu::{
    NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SHIFT_MASK, NEOMACS_SUPER_MASK,
};
use crate::thread_comm::InputEvent;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
// `key_without_modifiers` (the layout base key, before macOS Option composition
// / shift): the winit equivalent of GNU's `charactersIgnoringModifiers`.
// Available on macOS, X11, and Wayland — the platforms neomacs targets.
use neovm_host_abi::frontend_event::FrontendLogicalExtent;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::WindowId;

impl RenderApp {
    fn handle_render_popup_key(
        render: &mut GuiFrameRenderState,
        logical_key: &Key,
    ) -> Option<InputEvent> {
        match logical_key.as_ref() {
            Key::Named(NamedKey::Escape) => {
                render.dismiss_all_chrome_menus();
                Some(InputEvent::MenuSelection { index: -1 })
            }
            Key::Named(NamedKey::ArrowDown) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                if menu.move_hover(1) {
                    render.mark_dirty();
                }
                None
            }
            Key::Named(NamedKey::ArrowUp) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                if menu.move_hover(-1) {
                    render.mark_dirty();
                }
                None
            }
            Key::Named(NamedKey::Enter) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                let panel = menu.active_panel();
                let hi = panel.hover_index;
                if hi >= 0 && (hi as usize) < panel.item_indices.len() {
                    let global_idx = panel.item_indices[hi as usize];
                    if menu.all_items[global_idx].submenu {
                        if menu.open_submenu() {
                            render.mark_dirty();
                        }
                        None
                    } else {
                        render.dismiss_all_chrome_menus();
                        Some(InputEvent::MenuSelection {
                            index: global_idx as i32,
                        })
                    }
                } else {
                    render.dismiss_all_chrome_menus();
                    Some(InputEvent::MenuSelection { index: -1 })
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                if menu.open_submenu() {
                    render.mark_dirty();
                }
                None
            }
            Key::Named(NamedKey::ArrowLeft) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                if menu.close_submenu() {
                    render.mark_dirty();
                }
                None
            }
            Key::Named(NamedKey::Home) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                menu.active_panel_mut().hover_index = -1;
                if menu.move_hover(1) {
                    render.mark_dirty();
                }
                None
            }
            Key::Named(NamedKey::End) => {
                let menu = render.overlays.popup_menu.as_mut()?;
                let len = menu.active_panel().item_indices.len() as i32;
                menu.active_panel_mut().hover_index = len;
                if menu.move_hover(-1) {
                    render.mark_dirty();
                }
                None
            }
            _ => None,
        }
    }

    fn emacs_frame_for_window_event(&self, window_id: WindowId) -> u64 {
        self.frame_windows
            .event_frame_for_winit(window_id)
            .unwrap_or(0)
    }

    fn record_typing_speed_keypress(&mut self, window_id: WindowId) {
        if !self.effects.typing_speed.enabled {
            return;
        }
        let now = std::time::Instant::now();
        if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
            window_state.render.record_typing_keypress(now);
        }
    }

    pub(super) fn record_idle_dim_activity(&mut self, window_id: WindowId) {
        if !self.effects.idle_dim.enabled {
            return;
        }
        let now = std::time::Instant::now();
        if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
            window_state.render.record_idle_activity(now);
        }
    }

    pub(super) fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.lifecycle_flags.shutdown_requested {
            tracing::debug!(
                "Dropping window event after shutdown requested: {:?}",
                event
            );
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                let is_primary = self.frame_windows.is_primary_winit(window_id);
                let emacs_fid = self.emacs_frame_for_window_event(window_id);
                self.comms
                    .send_input(InputEvent::close_requested(emacs_fid));
                if is_primary {
                    self.lifecycle_flags.shutdown_requested = true;
                    event_loop.exit();
                } else {
                    self.frame_windows.request_destroy(emacs_fid);
                }
            }

            WindowEvent::Destroyed => {
                let is_primary = self.frame_windows.is_primary_winit(window_id);
                let emacs_fid = self.emacs_frame_for_window_event(window_id);
                tracing::info!(
                    "Window destroyed: winit={:?} emacs_frame_id=0x{:x} primary={}",
                    window_id,
                    emacs_fid,
                    is_primary
                );
                if is_primary {
                    self.lifecycle_flags.shutdown_requested = true;
                    event_loop.exit();
                } else {
                    self.frame_windows.request_destroy(emacs_fid);
                }
            }

            WindowEvent::Resized(size) => {
                tracing::info!("WindowEvent::Resized: {}x{}", size.width, size.height);

                let emacs_fid = self.emacs_frame_for_window_event(window_id);
                let is_primary = self.frame_windows.is_primary_winit(window_id);
                if let Some(device) = self.gpu.as_ref().map(|gpu| gpu.device.clone())
                    && let Some(ws) = self.frame_windows.get_by_winit_mut(window_id)
                {
                    ws.handle_resize(&device, size.width, size.height);
                    if is_primary {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.resize(size.width, size.height);
                        }
                        if self.effects.resize_padding.enabled
                            && let Some(renderer) = self.renderer.as_ref()
                        {
                            renderer.trigger_transient_resize_padding(
                                &mut ws.render.compositor.renderer_effects,
                                std::time::Instant::now(),
                            );
                        }
                        ws.render.mark_dirty();
                    }
                    let scale_factor = ws.scale_factor();
                    let (emacs_w, emacs_h) =
                        emacs_pixels_from_window_size(size.width, size.height, scale_factor);
                    self.comms.send_input(
                        InputEvent::viewport_changed(
                            FrontendLogicalExtent::new(emacs_w, emacs_h),
                            scale_factor,
                            emacs_fid,
                        )
                        .expect("winit supplies a valid display scale"),
                    );
                }
            }

            WindowEvent::Focused(focused) => {
                let emacs_fid = self.emacs_frame_for_window_event(window_id);
                if let Some(sched_id) = self.frame_windows.event_frame_for_winit(window_id) {
                    self.frame_coordinator
                        .set_focused(super::frame_sched::NativeWindowId(sched_id), focused);
                }
                let retirements = if focused {
                    Vec::new()
                } else {
                    self.frame_windows
                        .get_by_winit_mut(window_id)
                        .map(|window| window.render.cancel_pointer_interaction().1)
                        .unwrap_or_default()
                };
                self.comms
                    .send_input(InputEvent::focus_changed(focused, emacs_fid));
                for presentation in retirements {
                    self.comms
                        .send_input(InputEvent::PresentationRetired { presentation });
                }
            }

            WindowEvent::Occluded(occluded) => {
                // Occlusion is scheduling input: an occluded window presents
                // nothing; exposure issues exactly one recovery frame. On
                // Wayland this event also stands in for hidden/minimized,
                // where frame callbacks would otherwise stop delivering.
                if let Some(sched_id) = self.frame_windows.event_frame_for_winit(window_id) {
                    let id = super::frame_sched::NativeWindowId(sched_id);
                    let action = self.frame_coordinator.set_occluded(id, occluded);
                    if action == super::frame_sched::PacingAction::RequestRedraw
                        && let Some(window_state) = self.frame_windows.get(sched_id)
                    {
                        window_state.request_redraw();
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                // macOS composes Option+key at the OS level (Option+x -> ≈), so
                // winit's `logical_key` is the composed character. When a
                // command modifier is active we want the layout base key
                // (M-x, not M-≈), mirroring GNU's `charactersIgnoringModifiers`.
                // Capture it before destructuring the event.
                let key_without_mods = event.key_without_modifiers();
                let KeyEvent {
                    logical_key,
                    state,
                    text,
                    physical_key,
                    ..
                } = event;
                #[cfg(feature = "webview")]
                if let Some(target) = self.focused_webview {
                    use winit::platform::scancode::PhysicalKeyExtScancode;

                    let key_value = Self::translate_key(&logical_key);
                    if key_value != 0
                        && let Some(system) = self.webview_system.as_mut()
                    {
                        let input = neomacs_webview::WebViewInput::Keyboard {
                            key_value,
                            hardware_key_code: physical_key.to_scancode().unwrap_or(0),
                            state: match state {
                                ElementState::Pressed => neomacs_webview::ButtonState::Pressed,
                                ElementState::Released => neomacs_webview::ButtonState::Released,
                            },
                            modifiers: Self::webview_modifiers(self.modifiers),
                        };
                        if let Err(error) = system.input(target, input) {
                            tracing::warn!(view = %target.view(), %error, "dropping WebView keyboard input");
                        }
                    }
                }
                if state == ElementState::Pressed {
                    tracing::debug!(
                        "KeyboardInput: logical_key={:?} physical_key={:?} text={:?} mods={} ime={}",
                        logical_key,
                        physical_key,
                        text,
                        self.modifiers,
                        self.frame_windows
                            .primary_window()
                            .is_some_and(|ws| ws.render.has_ime_preedit())
                    );
                }
                let is_primary = self.frame_windows.is_primary_winit(window_id);
                let ime_preedit_active = self.frame_windows.get_by_winit(window_id).map_or_else(
                    || {
                        is_primary
                            && self
                                .frame_windows
                                .primary_window()
                                .is_some_and(|ws| ws.render.has_ime_preedit())
                    },
                    |ws| ws.render.has_ime_preedit(),
                );
                let handled_managed_popup = state == ElementState::Pressed
                    && self
                        .frame_windows
                        .get_by_winit(window_id)
                        .is_some_and(|ws| ws.render.overlays.popup_menu.is_some());
                if handled_managed_popup {
                    let event = self
                        .frame_windows
                        .get_by_winit_mut(window_id)
                        .and_then(|ws| Self::handle_render_popup_key(&mut ws.render, &logical_key));
                    if let Some(event) = event {
                        self.comms.send_input(event);
                    }
                } else if self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.render.overlays.popup_menu.as_ref())
                    .is_some()
                    && state == ElementState::Pressed
                {
                    let event = self
                        .frame_windows
                        .primary_window_mut()
                        .map(|ws| &mut ws.render)
                        .and_then(|render| Self::handle_render_popup_key(render, &logical_key));
                    if let Some(event) = event {
                        self.comms.send_input(event);
                    }
                } else if ime_preedit_active {
                    tracing::debug!(
                        "IME preedit active, suppressing KeyboardInput: {:?}",
                        logical_key
                    );
                } else {
                    let mut handled_via_text = false;
                    if state == ElementState::Pressed
                        && Self::should_use_committed_text(&logical_key)
                        && let Some(ref txt) = text
                    {
                        let s = txt.as_str();
                        if let Some(control_keysym) = Self::translate_control_text(s) {
                            tracing::debug!(
                                "KeyboardInput control text path: text={:?} keysym=0x{:04x} mods=0x{:x}",
                                s,
                                control_keysym,
                                self.modifiers
                            );
                            self.comms.send_input(InputEvent::key(
                                control_keysym,
                                self.modifiers,
                                true,
                                self.emacs_frame_for_window_event(window_id),
                            ));
                            self.record_idle_dim_activity(window_id);
                            self.record_typing_speed_keypress(window_id);
                            handled_via_text = true;
                        } else if let Some(keysyms) =
                            Self::translate_committed_text(s, self.modifiers)
                        {
                            tracing::debug!(
                                "KeyboardInput committed text path: text={:?} keysyms={:?} mods=0x{:x}",
                                s,
                                keysyms,
                                self.modifiers
                            );
                            for keysym in keysyms {
                                tracing::debug!(
                                    "Queueing text key event: keysym=0x{:04x} mods=0x{:x}",
                                    keysym,
                                    self.modifiers
                                );
                                self.comms.send_input(InputEvent::key(
                                    keysym,
                                    self.modifiers,
                                    true,
                                    self.emacs_frame_for_window_event(window_id),
                                ));
                                self.record_idle_dim_activity(window_id);
                                self.record_typing_speed_keypress(window_id);
                            }
                            handled_via_text = true;
                        }
                    }
                    if !handled_via_text {
                        // With a command modifier (Ctrl/Meta/Super) and no
                        // shift, translate the layout BASE key so a macOS
                        // Option-composed character (Option+x -> ≈) resolves to
                        // M-x, not M-≈ (issue: no M-x on macOS). GNU does the
                        // same via `charactersIgnoringModifiers`. Off macOS and
                        // for un-composed keys `key_without_modifiers` equals
                        // `logical_key`, so this is a no-op there; the
                        // shift+command case keeps the composed key (winit's
                        // base strips shift too, which M-x itself never needs).
                        let command_like = self.modifiers
                            & (NEOMACS_CTRL_MASK | NEOMACS_META_MASK | NEOMACS_SUPER_MASK)
                            != 0;
                        let effective_key =
                            if command_like && self.modifiers & NEOMACS_SHIFT_MASK == 0 {
                                &key_without_mods
                            } else {
                                &logical_key
                            };
                        let mut keysym = Self::translate_key(effective_key);
                        if keysym == 0 && self.modifiers != 0 {
                            use winit::keyboard::KeyCode;
                            use winit::keyboard::PhysicalKey;
                            keysym = match physical_key {
                                PhysicalKey::Code(KeyCode::Space) => 0x20,
                                _ => 0,
                            };
                        }
                        if keysym != 0 {
                            tracing::debug!(
                                "KeyboardInput translated path: logical_key={:?} physical_key={:?} keysym=0x{:04x} mods=0x{:x} pressed={}",
                                logical_key,
                                physical_key,
                                keysym,
                                self.modifiers,
                                state == ElementState::Pressed
                            );
                            if state == ElementState::Pressed
                                && let Some(window_state) =
                                    self.frame_windows.get_by_winit_mut(window_id)
                            {
                                window_state.set_mouse_hidden_for_typing(true);
                            }
                            if state == ElementState::Pressed {
                                self.record_typing_speed_keypress(window_id);
                            }
                            if self.effects.idle_dim.enabled {
                                self.record_idle_dim_activity(window_id);
                            }
                            self.comms.send_input(InputEvent::key(
                                keysym,
                                self.modifiers,
                                state == ElementState::Pressed,
                                self.emacs_frame_for_window_event(window_id),
                            ));
                        } else if state == ElementState::Pressed {
                            tracing::debug!(
                                "KeyboardInput dropped after translation: logical_key={:?} physical_key={:?} text={:?} mods=0x{:x}",
                                logical_key,
                                physical_key,
                                text,
                                self.modifiers
                            );
                        }
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(window_id, state, button);
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(window_id, position);
            }

            WindowEvent::CursorLeft { .. } => {
                self.handle_cursor_left(window_id);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(window_id, delta);
            }

            WindowEvent::RedrawRequested => {
                super::frame_stats::count(&super::frame_stats::REDRAW_EVENTS);
                if let Some(emacs_fid) = self.frame_windows.event_frame_for_winit(window_id) {
                    use super::frame_sched::{
                        ClockSource, FrameTick, NativeWindowId, PacingAction,
                    };
                    let sched_id = NativeWindowId(emacs_fid);
                    let now = std::time::Instant::now();
                    let estimated_interval = self
                        .frame_windows
                        .get(emacs_fid)
                        .map(|window_state| {
                            std::time::Duration::from_secs_f64(
                                1.0 / f64::from(Self::window_max_rate(window_state).get()),
                            )
                        })
                        .unwrap_or(std::time::Duration::from_millis(16));
                    let tick = FrameTick {
                        frame_time: now,
                        target_presentation_time: now + estimated_interval,
                        estimated_interval,
                        source: ClockSource::Synthetic,
                    };
                    let mut plan = self.frame_coordinator.begin_frame(sched_id, tick);
                    // Occluded/hidden windows present nothing. begin_frame
                    // already returns no work while ineligible; skipping the
                    // render here makes that concrete and avoids servicing an
                    // OS-delivered RedrawRequested for a surface the
                    // compositor is not showing.
                    if !self.frame_coordinator.is_eligible(sched_id) {
                        super::frame_stats::count_plan(sched_id, &plan);
                        return;
                    }
                    if !plan.should_present {
                        // Nothing we scheduled explains this tick, and this is
                        // the one place that knows why: the event came from the
                        // window system (expose, resize, first map) or from a
                        // recovery path that requests a redraw on the window
                        // directly. Both need the surface repainted, so the
                        // frame is granted under a reason of its own rather
                        // than rendered unattributed.
                        plan = self.frame_coordinator.platform_redraw_plan(sched_id, tick);
                    }
                    super::frame_stats::count_plan(sched_id, &plan);
                    if plan.reasons.is_empty() {
                        super::frame_stats::count(
                            &super::frame_stats::UNATTRIBUTED_PRESENT_ATTEMPTS,
                        );
                        debug_assert!(
                            false,
                            "every presented frame must name a demand reason (invariant 12)"
                        );
                    }
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_frame_sample_time(plan.tick.target_presentation_time);
                    }
                    if let Some(window_state) = self.frame_windows.get_mut(emacs_fid) {
                        window_state.render.set_dirty(false);
                    }
                    // Stage 4: any compositor-only plan can use the retained
                    // static scene and sample just its dynamic layers (cursor,
                    // frame post, and later compositor effects). Any stronger
                    // work class renders fully.
                    let compositor_only = matches!(
                        plan.work,
                        super::frame_sched::RenderWork::CompositeOnly { .. }
                    );
                    let result = self.render_frame_window_hinted(emacs_fid, compositor_only);
                    let action = self.frame_coordinator.finish_frame(
                        sched_id,
                        &plan,
                        result,
                        std::time::Instant::now(),
                    );
                    if action == PacingAction::RequestRedraw
                        && let Some(window_state) = self.frame_windows.get(emacs_fid)
                    {
                        window_state.request_redraw();
                    }
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                let old_modifiers = self.modifiers;
                let state = mods.state();
                self.modifiers = 0;
                if state.shift_key() {
                    self.modifiers |= NEOMACS_SHIFT_MASK;
                }
                if state.control_key() {
                    self.modifiers |= NEOMACS_CTRL_MASK;
                }
                if state.alt_key() {
                    self.modifiers |= NEOMACS_META_MASK;
                }
                if state.super_key() {
                    self.modifiers |= NEOMACS_SUPER_MASK;
                }
                tracing::debug!(
                    "ModifiersChanged: old=0x{:x} new=0x{:x} shift={} ctrl={} alt={} super={}",
                    old_modifiers,
                    self.modifiers,
                    state.shift_key(),
                    state.control_key(),
                    state.alt_key(),
                    state.super_key()
                );
                if self.modifiers != old_modifiers {
                    self.record_idle_dim_activity(window_id);
                }
            }

            WindowEvent::Ime(ime_event) => match ime_event {
                winit::event::Ime::Enabled => {
                    if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
                        window_state.set_ime_enabled(true);
                        window_state.reset_ime_cursor_area();
                        if let Some(target) = window_state.render.cursor.target_cloned() {
                            Self::update_frame_window_ime_cursor_area_if_needed(
                                window_state,
                                &target,
                            );
                        }
                    } else if self.frame_windows.is_primary_winit(window_id) {
                        if let Some(window_state) = self.frame_windows.primary_window_mut() {
                            window_state.set_ime_enabled(true)
                        };
                        if let Some(window_state) = self.frame_windows.primary_window_mut() {
                            window_state.reset_ime_cursor_area()
                        };
                        if let Some(target) = self
                            .frame_windows
                            .primary_window()
                            .map_or(&self.cursor_defaults, |ws| &ws.render.cursor)
                            .target_cloned()
                        {
                            self.update_ime_cursor_area_if_needed(&target);
                        }
                    }
                    tracing::info!("IME enabled");
                }
                winit::event::Ime::Disabled => {
                    if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
                        window_state.set_ime_enabled(false);
                        window_state.clear_ime_preedit();
                    } else if self.frame_windows.is_primary_winit(window_id) {
                        if let Some(window_state) = self.frame_windows.primary_window_mut() {
                            window_state.set_ime_enabled(false)
                        };
                        if let Some(ws) = self.frame_windows.primary_window_mut() {
                            ws.render.clear_ime_preedit()
                        };
                        if let Some(window_state) = self.frame_windows.primary_window_mut() {
                            window_state.reset_ime_cursor_area()
                        };
                    }
                    tracing::info!("IME disabled");
                }
                winit::event::Ime::Commit(text) => {
                    tracing::debug!("IME Commit: '{}'", text);
                    if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
                        window_state.render.clear_ime_preedit();
                    } else if self.frame_windows.is_primary_winit(window_id)
                        && let Some(ws) = self.frame_windows.primary_window_mut()
                    {
                        ws.render.clear_ime_preedit()
                    };
                    if !text.is_empty() {
                        self.comms.send_input(InputEvent::text_committed(
                            text,
                            self.emacs_frame_for_window_event(window_id),
                        ));
                        self.record_idle_dim_activity(window_id);
                        self.record_typing_speed_keypress(window_id);
                    }
                }
                winit::event::Ime::Preedit(text, cursor_range) => {
                    tracing::debug!("IME Preedit: '{}' cursor: {:?}", text, cursor_range);
                    if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
                        window_state
                            .render
                            .set_ime_preedit(text.clone(), cursor_range);
                        if let Some(target) = window_state.render.cursor.target_cloned() {
                            Self::update_frame_window_ime_cursor_area_if_needed(
                                window_state,
                                &target,
                            );
                        }
                    } else if self.frame_windows.is_primary_winit(window_id) {
                        if let Some(ws) = self.frame_windows.primary_window_mut() {
                            ws.render.set_ime_preedit(text.clone(), cursor_range)
                        };
                        if let Some(target) = self
                            .frame_windows
                            .primary_window()
                            .map_or(&self.cursor_defaults, |ws| &ws.render.cursor)
                            .target_cloned()
                        {
                            self.update_ime_cursor_area_if_needed(&target);
                        }
                    }
                }
            },

            WindowEvent::DroppedFile(path) => {
                if let Some(path_str) = path.to_str() {
                    tracing::info!("File dropped: {}", path_str);
                    let mouse_pos = self
                        .frame_windows
                        .get_by_winit(window_id)
                        .map_or((0.0, 0.0), |window_state| window_state.render.mouse_pos);
                    self.comms.send_input(InputEvent::FileDrop {
                        paths: vec![path_str.to_string()],
                        x: mouse_pos.0,
                        y: mouse_pos.1,
                    });
                }
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let effective_scale = effective_window_scale_factor(scale_factor);
                let is_primary = self.frame_windows.is_primary_winit(window_id);
                if let Some(ws) = self.frame_windows.get_by_winit_mut(window_id) {
                    tracing::info!(
                        "Scale factor changed for frame 0x{:x}: previous_effective={} raw={} effective={}",
                        ws.render.emacs_frame_id,
                        ws.scale_factor(),
                        scale_factor,
                        effective_scale
                    );
                    ws.set_scale_factor(scale_factor);
                    if is_primary && let Some(ref mut renderer) = self.renderer {
                        renderer.set_scale_factor(effective_scale as f32);
                    }
                    let (native_width, native_height) = ws.native_size();
                    let (width, height) =
                        emacs_pixels_from_window_size(native_width, native_height, effective_scale);
                    self.comms.send_input(
                        InputEvent::viewport_changed(
                            FrontendLogicalExtent::new(width, height),
                            effective_scale,
                            ws.render.emacs_frame_id,
                        )
                        .expect("winit supplies a valid display scale"),
                    );
                }
            }

            _ => {}
        }
    }
}
