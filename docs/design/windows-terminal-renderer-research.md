# Windows terminal renderer research

Research date: 2026-09-09. Inspected published `termwiz 0.23.3`, `crossterm 0.29.0`, and `crossterm_winapi 0.9.1`. No implementation or Windows runtime validation was performed.

## Recommendation

Keep the existing capability-driven byte renderer for terminals whose output handle actually accepts virtual-terminal processing. For a legacy Windows console, render structured cell runs using the existing crossterm ecosystem: crossterm's native cursor/clear commands and the safe `crossterm_winapi` console/handle wrappers. Select 16,777,216 versus 16 colors from the negotiated output mode, rather than inventing a terminfo entry. This is a proposed integration, not a claim that crossterm implements GNU's complete policy automatically.

The natural boundary is before `TtyRif::encode_ops`, where `TermOp` and the desired grid still carry structure. The current `take_output() -> Vec<u8>` has already erased that structure. Writing those bytes through a different `Write` implementation does not invoke crossterm's native fallback. For the first legacy backend, repaint dirty desired rows, bypassing scroll/insert/delete optimizations that have no equivalent implementation yet. Keep the screen model consistent with the operation actually sent, and retain damage after failed output.

## Why not adopt termwiz's WindowsTerminal directly?

The public constructors are `WindowsTerminal::new(Capabilities)`, `new_from_stdio(Capabilities)`, and `new_with<A: Read + IsTty + AsRawHandle, B: Write + IsTty + AsRawHandle>(caps, read, write)`. `new` opens `CONIN$` and `CONOUT$`; `new_with` duplicates handles into owned descriptors. `Terminal::render(&[Change])` accepts structured changes, and `Terminal::flush()` writes buffered output. The constructor attempts `ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN`, selecting `TerminfoRenderer` on success and `WindowsConsoleRenderer` otherwise, unless an explicitly provided terminfo database takes precedence. [Versioned Windows implementation](https://docs.rs/crate/termwiz/0.23.3/source/src/terminal/windows.rs), [public Terminal API](https://docs.rs/termwiz/0.23.3/termwiz/terminal/trait.Terminal.html).

However, **native alternate-screen entry and exit are TODO no-ops** in this release (`windows.rs:722-757`). Drop restores modes/code pages, but includes `unwrap`/`expect` on restoration. Its input processing and mode ownership would also overlap Neomacs's existing input lifecycle. These make it unsuitable as a small, complete legacy-screen integration. [Lifecycle implementation](https://docs.rs/crate/termwiz/0.23.3/source/src/terminal/windows.rs).

The standalone `WindowsConsoleRenderer::new(Capabilities)` and `render_to<B: ConsoleOutputHandle + Write>(&mut self, changes: &[Change], out: &mut B)` are public. Its output handle implementation is private, while the public `ConsoleOutputHandle` trait requires roughly fifteen native operations, including screen-buffer access and scrolling. Implementing that trait ourselves would recreate considerable Windows interoperability. [Renderer](https://docs.rs/crate/termwiz/0.23.3/source/src/render/windows.rs), [handle trait](https://docs.rs/crate/termwiz/0.23.3/source/src/terminal/windows.rs).

`Change` supports `CursorPosition`, `AllAttributes`, `Text`, clear operations and region scrolling. It has no direct character insertion/deletion variants. Feeding desired cell runs is feasible, but does not eliminate the integration and lifecycle problems above. [Change definitions](https://docs.rs/crate/termwiz/0.23.3/source/src/surface/change.rs).

There is also no small Windows-renderer-only feature. Even without optional image/widgets/serde support, termwiz unconditionally depends on terminfo, parser/regex packages, Unicode packages, and several WezTerm crates. A Windows-only dependency declaration limits the target scope but does not remove that dependency graph. [Published manifest](https://docs.rs/crate/termwiz/0.23.3/source/Cargo.toml).

## Existing crossterm capabilities and gaps

Crossterm `Command::execute_winapi()` is a public Windows-only method. `QueueableCommand::queue()` normally selects native execution when ANSI is unavailable, flushing its writer first to preserve ordering. Commands must remain structured until this dispatch; a byte vector containing escape sequences bypasses it. [Command implementation](https://docs.rs/crate/crossterm/0.29.0/source/src/command.rs).

Useful public commands include `cursor::MoveTo`, cursor visibility commands, and `terminal::Clear`. In an explicitly selected legacy backend, calling the native command method avoids a second, contradictory backend decision. Crossterm's `supports_ansi()` is insufficient as the GNU color-depth gate: it returns true if VT enabling fails but `TERM` is present and not `dumb`. [Detection implementation](https://docs.rs/crate/crossterm/0.29.0/source/src/ansi_support.rs).

`style::SetAttribute`'s native implementation is a no-op. Its native color conversion maps `Color::Rgb` and `Color::AnsiValue` to zero; legacy output must map realized indices 0–15 to the named colors or directly to console attribute bits. Do not assume passing RGB or an indexed color performs palette quantization. [Style commands](https://docs.rs/crate/crossterm/0.29.0/source/src/style.rs), [native colors](https://docs.rs/crate/crossterm/0.29.0/source/src/style/sys/windows.rs).

The existing lower-level dependency supplies safe public APIs:

- `ScreenBuffer::current()`, `ScreenBuffer::create()`, `ScreenBuffer::show()`, `ScreenBuffer::info()`, and `ScreenBuffer::handle()`.
- `Console::from(handle).set_text_attribute(u16)` for a complete native attribute word.
- `Console::write_char_buffer(&[u8])`, which validates UTF-8 and uses `WriteConsoleW` after conversion.
- `Console::fill_whit_attribute` and `fill_whit_character` for clearing. The latter uses the narrow character API, so use it for spaces only, not general Unicode text.
- `ConsoleMode::from(handle)`, `.mode()`, and `.set_mode(u32)` for saving and negotiating the actual handle's mode.

[Screen-buffer source](https://docs.rs/crate/crossterm_winapi/0.9.1/source/src/screen_buffer.rs), [Console source](https://docs.rs/crate/crossterm_winapi/0.9.1/source/src/console.rs), [ConsoleMode source](https://docs.rs/crate/crossterm_winapi/0.9.1/source/src/console_mode.rs).

## Concrete lifecycle and output design

Use a Windows-only session object owning the original `ScreenBuffer` and the newly created alternate `ScreenBuffer`. Save the original modes and attributes. Activate the alternate buffer, retain both handles throughout the session, and restore the original with `show()` before closing the alternate handle. The wrapper's owned handles use `Arc` and close on final drop; standard handles are non-owning. Never reacquire `CONOUT$` and assume it still denotes the original screen: it denotes whichever buffer is active then. [Handle ownership](https://docs.rs/crate/crossterm_winapi/0.9.1/source/src/handle.rs).

Render a structured row run by moving to its viewport-relative destination, applying one native attribute word per attribute run, and sending valid UTF-8 through `Console::write_char_buffer`. The adapter should compute GNU's 16-color mapping and supported legacy attributes explicitly from `CellAttrs`; it should not reinterpret ANSI bytes or claim unsupported italic/strike/underline variants. Use the original buffer's attributes for default colors. Preserve grapheme text from the desired grid rather than converting cells individually to `u16`.

This needs no new application-owned unsafe for the APIs listed. It still requires Windows runtime checks for alternate-buffer restoration, viewport offsets, right-margin writes, wide/combining text, and failure cleanup. Crossterm is already used by Neomacs, but that fact is evidence of reuse, not proof that a new integration has been validated.
