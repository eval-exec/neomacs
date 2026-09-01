//! Terminal render commands.

#[cfg(feature = "neo-term")]
use super::RenderApp;

#[cfg(feature = "neo-term")]
use crate::thread_comm::TerminalCommand;

#[cfg(feature = "neo-term")]
impl RenderApp {
    /// Schedule one render-preparation pass for renderer-owned terminal state.
    ///
    /// PTY wakeups normally provide this demand through `has_terminal_activity`,
    /// but commands which remove a view or only change its compositor placement
    /// cannot rely on a view-local dirty bit: the view may already be gone.
    fn invalidate_terminal_scene(&mut self) {
        self.frame_windows.mark_top_level_dirty();
    }

    pub(super) fn handle_terminal(&mut self, cmd: TerminalCommand) {
        match cmd {
            TerminalCommand::TerminalCreate {
                id,
                size,
                target,
                shell,
            } => match crate::terminal::TerminalView::new(id, size, target, shell.as_deref()) {
                Ok(view) => {
                    if let Err(error) = self.shared_terminals.mark_live(id, view.term.clone()) {
                        tracing::error!("Failed to publish terminal {id}: {error}");
                        return;
                    }
                    self.terminal_manager.terminals.insert(id, view);
                    tracing::info!(
                        "Terminal {} created ({}x{}, {:?})",
                        id,
                        size.cols,
                        size.rows,
                        target
                    );
                }
                Err(e) => {
                    let error = e.to_string();
                    self.shared_terminals.mark_failed(id, error.clone());
                    self.comms
                        .send_input(crate::thread_comm::InputEvent::TerminalCreateFailed {
                            id,
                            error: error.clone(),
                        });
                    tracing::error!("Failed to create terminal {}: {}", id, e);
                }
            },
            TerminalCommand::TerminalWrite { id, data } => {
                if let Some(view) = self.terminal_manager.get_mut(id)
                    && let Err(e) = view.write(&data)
                {
                    tracing::warn!("Terminal {} write error: {}", id, e);
                }
            }
            TerminalCommand::TerminalResize { id, size } => {
                if let Some(view) = self.terminal_manager.get_mut(id) {
                    view.resize(size);
                }
            }
            TerminalCommand::TerminalDestroy { id } => {
                let was_present = self.terminal_manager.get(id).is_some();
                let result = self.terminal_manager.destroy(id);
                if was_present {
                    // `destroy` removes the view before shutting its PTY down,
                    // including on teardown failure. Ensure the retained scene
                    // is rebuilt from the manager's new authoritative state.
                    self.invalidate_terminal_scene();
                }
                match result {
                    Ok(true) => {
                        self.shared_terminals.complete_destroy(id);
                        tracing::info!("Terminal {} destroyed", id);
                    }
                    Ok(false) => {
                        self.shared_terminals.complete_destroy(id);
                        tracing::debug!("Terminal {} was already absent", id);
                    }
                    Err(error) => {
                        self.shared_terminals
                            .mark_destroy_failed(id, format!("teardown failed: {error}"));
                        tracing::error!("Terminal {} teardown failed: {}", id, error);
                    }
                }
            }
            TerminalCommand::TerminalSetFloat { id, placement } => {
                if let Some(view) = self.terminal_manager.get_mut(id) {
                    let next = (placement.x(), placement.y(), placement.opacity());
                    let changed = (view.float_x, view.float_y, view.float_opacity) != next;
                    if changed {
                        (view.float_x, view.float_y, view.float_opacity) = next;
                        self.invalidate_terminal_scene();
                    }
                }
            }
        }
    }
}

#[cfg(all(test, feature = "neo-term"))]
#[path = "terminal_commands_test.rs"]
mod tests;
