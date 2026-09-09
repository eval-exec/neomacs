use super::RenderApp;
use super::state::{effective_window_scale_factor, emacs_pixels_from_window_size};
use crate::backend::wgpu::{
    NEOMACS_CTRL_MASK, NEOMACS_META_MASK, NEOMACS_SHIFT_MASK, NEOMACS_SUPER_MASK,
};
use crate::thread_comm::InputEvent;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

impl RenderApp {
    fn emacs_frame_for_window_event(&self, window_id: WindowId) -> u64 {
        self.frame_windows
            .event_frame_for_winit(window_id)
            .unwrap_or(0)
    }

    fn record_typing_speed_keypress(&mut self, window_id: WindowId) {
        if !self.effects.typing_speed.enabled {
            return;
        }
        let now = neomacs_display_protocol::frame_time::observe_platform_now();
        if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
            window_state.render.record_typing_keypress(now);
        }
    }

    pub(super) fn record_idle_dim_activity(&mut self, window_id: WindowId) {
        if !self.effects.idle_dim.enabled {
            return;
        }
        let now = neomacs_display_protocol::frame_time::observe_platform_now();
        if let Some(window_state) = self.frame_windows.get_by_winit_mut(window_id) {
            window_state.render.record_idle_activity(now);
        }
    }

    pub(super) fn handle_window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            if self
                .tooltips
                .event(window_id, &event, &gpu.device, &gpu.queue, renderer)
            {
                return;
            }
        }
        let live_owner = self.frame_windows.get_by_winit(window_id).is_some();
        let dismiss = match &event {
            WindowEvent::PointerButton {
                state: ElementState::Pressed,
                ..
            }
            | WindowEvent::MouseWheel { .. } => live_owner,
            WindowEvent::KeyboardInput { event, .. } => {
                live_owner && event.state == ElementState::Pressed
            }
            WindowEvent::PointerLeft { .. } => {
                self.tooltips.owns_parent(window_id)
                    || (live_owner && self.tooltips.owner().is_none())
            }
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.tooltips.owns_parent(window_id)
            }
            _ => false,
        };
        if dismiss {
            self.comms.tooltip_context.invalidate();
            self.tooltips.hide();
        }
        if self.handle_native_menu_bar_event(window_id, &event)
            || self.handle_native_menu_bar_key(window_id, &event)
        {
            return;
        }
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            if self
                .menus
                .event(window_id, &event, &gpu.device, &gpu.queue, renderer)
            {
                if let Err(error) = self.menus.sync(
                    event_loop,
                    &gpu.instance,
                    &gpu.adapter,
                    &gpu.device,
                    &gpu.queue,
                    renderer.surface_format(),
                ) {
                    tracing::error!(%error, "native menu update failed");
                    self.menus.cancel();
                }
                while let Some(index) = self.menus.take_result() {
                    self.comms.send_input(InputEvent::MenuSelection {
                        index: index.index(),
                        token: Some(index.token),
                    });
                }
                self.sync_menu_heading();
                return;
            }
        }
        // Ignore late events from destroyed native popups or frame windows.
        if self.frame_windows.get_by_winit(window_id).is_none() {
            return;
        }
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
                self.comms.send_input(InputEvent::WindowClose {
                    emacs_frame_id: emacs_fid,
                });
                if is_primary {
                    self.lifecycle_flags.shutdown_requested = true;
                    self.handle_exiting();
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
                    self.handle_exiting();
                    event_loop.exit();
                } else {
                    self.frame_windows.request_destroy(emacs_fid);
                }
            }

            WindowEvent::SurfaceResized(size) => {
                tracing::info!(
                    "WindowEvent::SurfaceResized: {}x{}",
                    size.width,
                    size.height
                );

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
                                neomacs_display_protocol::frame_time::observe_platform_now()
                                    .into_instant(),
                            );
                        }
                        ws.render.mark_dirty();
                    }
                    let scale_factor = ws.scale_factor();
                    let (emacs_w, emacs_h) =
                        emacs_pixels_from_window_size(size.width, size.height, scale_factor);
                    self.comms.send_input(InputEvent::WindowResize {
                        width: emacs_w,
                        height: emacs_h,
                        scale_factor,
                        emacs_frame_id: emacs_fid,
                    });
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
                    let menu_owns_focus = self.menus.owns_parent(window_id);
                    self.frame_windows
                        .get_by_winit_mut(window_id)
                        .map(|window| {
                            let active = window.render.chrome.interaction.menu_bar_active;
                            let compact = window.render.chrome.interaction.compact_bar_menu_active;
                            let retirements = window.render.cancel_pointer_interaction().1;
                            if menu_owns_focus {
                                // A native menu's keyboard grab must not erase
                                // the heading used for hover/click switching.
                                window.render.chrome.interaction.menu_bar_active = active;
                                window.render.chrome.interaction.compact_bar_menu_active = compact;
                            }
                            retirements
                        })
                        .unwrap_or_default()
                };
                self.comms.send_input(InputEvent::WindowFocus {
                    focused,
                    emacs_frame_id: emacs_fid,
                });
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
                if ime_preedit_active {
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
                            self.comms.send_input(InputEvent::Key {
                                keysym: control_keysym,
                                modifiers: self.modifiers,
                                pressed: true,
                                emacs_frame_id: self.emacs_frame_for_window_event(window_id),
                            });
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
                                self.comms.send_input(InputEvent::Key {
                                    keysym,
                                    modifiers: self.modifiers,
                                    pressed: true,
                                    emacs_frame_id: self.emacs_frame_for_window_event(window_id),
                                });
                                self.record_idle_dim_activity(window_id);
                                self.record_typing_speed_keypress(window_id);
                            }
                            handled_via_text = true;
                        }
                    }
                    if !handled_via_text {
                        // `logical_key` is the layout's key with Shift applied
                        // and no command modifier folded in, which is what GNU
                        // reads for a command chord. macOS is the one window
                        // system that would compose an Option chord into
                        // another character first, and the frame window asks
                        // it not to (`apply_option_key_policy`), so no
                        // per-event substitution is needed here.
                        let mut keysym = Self::translate_key(&logical_key);
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
                            self.comms.send_input(InputEvent::Key {
                                keysym,
                                modifiers: self.modifiers,
                                pressed: state == ElementState::Pressed,
                                emacs_frame_id: self.emacs_frame_for_window_event(window_id),
                            });
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

            WindowEvent::PointerButton {
                state,
                button,
                position,
                primary,
                ..
            } => {
                if primary {
                    self.handle_cursor_moved(window_id, position);
                    if let Some(button) = button.mouse_button() {
                        self.handle_mouse_input(window_id, state, button);
                    }
                }
            }

            WindowEvent::PointerMoved { position, .. } => {
                self.handle_cursor_moved(window_id, position);
            }

            WindowEvent::PointerLeft { .. } => {
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
                    let now = neomacs_display_protocol::frame_time::observe_platform_now();
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
                        target_presentation_time: now.plus(estimated_interval),
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
                        renderer.set_frame_sample(plan.tick.sample());
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
                        neomacs_display_protocol::frame_time::observe_platform_now(),
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
                if state.meta_key() {
                    self.modifiers |= NEOMACS_SUPER_MASK;
                }
                tracing::debug!(
                    "ModifiersChanged: old=0x{:x} new=0x{:x} shift={} ctrl={} alt={} super={}",
                    old_modifiers,
                    self.modifiers,
                    state.shift_key(),
                    state.control_key(),
                    state.alt_key(),
                    state.meta_key()
                );
                if self.modifiers != old_modifiers {
                    self.record_idle_dim_activity(window_id);
                }
            }

            WindowEvent::Ime(ime_event) => match ime_event {
                // Surrounding-text support is not advertised by our IME policy.
                winit::event::Ime::DeleteSurrounding { .. } => {}
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
                    for ch in text.chars() {
                        let keysym = ch as u32;
                        if keysym != 0 {
                            self.comms.send_input(InputEvent::Key {
                                keysym,
                                modifiers: 0,
                                pressed: true,
                                emacs_frame_id: self.emacs_frame_for_window_event(window_id),
                            });
                            self.record_idle_dim_activity(window_id);
                            self.record_typing_speed_keypress(window_id);
                        }
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

            WindowEvent::DragEntered { id, .. } => {
                use winit::data_transfer::TypeHint;
                use winit::event_loop::DndAction;
                if event_loop
                    .data_transfer(id)
                    .is_ok_and(|data| data.has_type(&TypeHint::UriList))
                {
                    let _ = event_loop.set_valid_dnd_actions(id, &[DndAction::Copy]);
                }
            }
            WindowEvent::DragDropped { id, .. } => {
                if let Ok(serial) =
                    event_loop.fetch_data_transfer(id, &winit::data_transfer::TypeHint::UriList)
                {
                    self.pending_file_drops.insert(serial);
                }
            }
            WindowEvent::DataTransferReceived { serial, value, .. } => {
                if self.pending_file_drops.remove(&serial) {
                    if let Ok(paths) = value.try_as_file_paths() {
                        let paths = paths
                            .into_iter()
                            .filter_map(|path| path.to_str().map(str::to_owned))
                            .collect::<Vec<_>>();
                        if !paths.is_empty() {
                            self.comms.send_input(InputEvent::FileDrop { paths });
                        }
                    }
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
                    self.comms.send_input(InputEvent::WindowResize {
                        width,
                        height,
                        scale_factor: effective_scale,
                        emacs_frame_id: ws.render.emacs_frame_id,
                    });
                }
            }

            _ => {}
        }
    }
}
