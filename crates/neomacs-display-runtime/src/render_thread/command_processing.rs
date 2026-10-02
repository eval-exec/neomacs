use super::RenderApp;
use crate::thread_comm::{ClipboardCommand, LifecycleCommand, RenderCommand, WindowCommand};

impl RenderApp {
    /// During connection-only/GPU startup, configuration and frame lifecycle
    /// live in CPU state. Other commands retain order in a bounded staging queue;
    /// stop draining when full, leaving producer admission explicitly fallible.
    pub(super) fn process_startup_commands(&mut self) -> bool {
        self.retire_cancelled_frames();
        while self.startup_commands.len() < 64 {
            let Ok(command) = self.comms.cmd_rx.try_recv() else {
                break;
            };
            if !self.comms.keep_alive_without_frames {
                if matches!(
                    command,
                    RenderCommand::Lifecycle(LifecycleCommand::Shutdown)
                ) {
                    self.lifecycle_flags
                        .request_shutdown(super::state::RenderShutdownReason::EvaluatorShutdown);
                    return true;
                }
                self.startup_commands.push_back(command);
                continue;
            }
            match command {
                RenderCommand::Lifecycle(LifecycleCommand::Shutdown) => {
                    self.lifecycle_flags
                        .request_shutdown(super::state::RenderShutdownReason::EvaluatorShutdown);
                    return true;
                }
                RenderCommand::Config(command) => self.handle_config(command),
                RenderCommand::Window(command) => self.handle_window(command),
                RenderCommand::Clipboard(command) => self.handle_clipboard(command),
                other => self.startup_commands.push_back(other),
            }
        }
        false
    }

    /// Process pending commands from Emacs.
    pub(super) fn process_commands(&mut self) -> bool {
        self.retire_cancelled_frames();
        let mut should_exit = false;

        while let Some(cmd) = self
            .startup_commands
            .pop_front()
            .or_else(|| self.comms.cmd_rx.try_recv().ok())
        {
            match cmd {
                RenderCommand::Lifecycle(c) => {
                    if let LifecycleCommand::Shutdown = c {
                        tracing::info!("Render thread received shutdown command");
                        self.lifecycle_flags.request_shutdown(
                            super::state::RenderShutdownReason::EvaluatorShutdown,
                        );
                        should_exit = true;
                        continue;
                    }
                }
                RenderCommand::Window(c) => {
                    if matches!(c, WindowCommand::ScrollBlit { .. }) {
                        tracing::debug!("ScrollBlit ignored (full-frame rendering mode)");
                        continue;
                    }
                    self.handle_window(c);
                }
                RenderCommand::Asset(c) => self.handle_asset(c),
                #[cfg(feature = "neo-term")]
                RenderCommand::Terminal(c) => self.handle_terminal(c),
                RenderCommand::Ui(c) => self.handle_ui(c),
                RenderCommand::Config(c) => self.handle_config(c),
                RenderCommand::Clipboard(c) => self.handle_clipboard(c),
            }
        }

        should_exit
    }

    fn handle_clipboard(&mut self, command: ClipboardCommand) {
        match &self.clipboard {
            Ok(clipboard) => clipboard.submit(command),
            Err(err) => crate::clipboard::reject_command(command, err.clone()),
        }
    }
}
