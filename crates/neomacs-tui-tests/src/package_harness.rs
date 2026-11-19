//! The screen vocabulary the interactive package suites share.
//!
//! Each package suite boots a GNU Emacs and a Neomacs into a PTY pair and
//! drives them by screen state, so the same few primitives -- wait for a row,
//! type a command, run one step against both editors and keep both failures --
//! were copied into every suite's own `harness.rs`. They live here instead:
//! one implementation, so a fix to a wait or a panic message reaches the whole
//! family rather than one package.
//!
//! A suite's `harness.rs` keeps what is genuinely its own: readiness markers,
//! package-specific capture, and the values it waits on. A wait's tolerance
//! stays at its call site -- `invoke_with_prompt_timeout` for a prompt that
//! follows a heavy screen -- so a suite can be patient where it has reason to
//! be without forking the helper.

use crate::TuiSession;
use crate::package_scenario::PackageTuiPair;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;
use std::time::Duration;

/// Re-read `session` until `predicate` holds, or fail with the final screen.
///
/// The timeout is explicit at each call site: how long a row may take is a
/// property of what is being waited for, not of the helper.
pub fn wait_for(
    session: &mut TuiSession,
    timeout: Duration,
    description: &str,
    predicate: impl Fn(&[String]) -> bool,
) {
    session.read_until(timeout, |grid| predicate(grid));
    let grid = session.text_grid();
    assert!(
        predicate(&grid),
        "{} timed out waiting for {description}:\n{}",
        session.name,
        grid.join("\n")
    );
}

/// Type `M-x <command> RET` into `session` and wait for the `ready` marker.
pub fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    invoke_with_prompt_timeout(session, command, ready, Duration::from_secs(8))
}

/// As [`invoke`], but with an explicit budget for the `M-x` prompt.
///
/// The prompt opens when the editor next reads keys, and an editor still
/// working through a large asynchronous screen -- Magit inserting a status
/// buffer runs several git processes and fontifies the result -- can take
/// longer than the default to get there, however quickly the screen itself
/// settles.
pub fn invoke_with_prompt_timeout(
    session: &mut TuiSession,
    command: &str,
    ready: &str,
    prompt_timeout: Duration,
) {
    session.send_keys("M-x");
    wait_for(session, prompt_timeout, "M-x prompt", |grid| {
        grid.iter().any(|row| row.contains("M-x"))
    });
    session.send(command.as_bytes());
    session.send_keys("RET");
    wait_for(session, Duration::from_secs(20), ready, |grid| {
        grid.iter().any(|row| row.contains(ready))
    });
}

/// The message text of a caught panic payload.
///
/// A payload thrown with `panic_any` carries no message. Stable Rust cannot
/// print a `dyn Any` value, so the report keeps what identity the payload
/// has, the way `prin1` prints an opaque object: `#<...>`. String-shaped
/// payloads -- `String`, `&str`, `Box<str>`, `Box<String>` -- print
/// verbatim.
///
/// The payload arrives inside the panic runtime's `Box<dyn Any + Send>`,
/// and asking that BOX for its `type_id` reports the box itself, whatever
/// the payload: every opaque report used to print as
/// `Box<dyn Any + Send>`, which identifies nothing. The identity therefore
/// reads through `&*payload`. A payload that is itself a caught panic
/// re-thrown -- a boxed `Box<dyn Any + Send>` -- unwraps a layer at a time,
/// so a re-thrown message reports as the message rather than as a box.
///
/// The unit type names itself, because one dependency in this tree panics
/// with `()` on purpose: `expect-test` prints the diff between the expect
/// file and the captured value on stdout and then `resume_unwind`s a `()`
/// precisely to keep the failure backtrace-free. A `()` reaching this
/// function is therefore not an opaque payload but a snapshot mismatch, and
/// the report says where the diff went instead of an identity that
/// identifies nothing.
pub fn panic_text(mut payload: Box<dyn Any + Send>) -> String {
    loop {
        if let Some(text) = payload.downcast_ref::<String>() {
            return text.clone();
        }
        if let Some(text) = payload.downcast_ref::<&str>() {
            return (*text).to_owned();
        }
        if let Some(text) = payload.downcast_ref::<Box<str>>() {
            return (**text).to_owned();
        }
        if let Some(text) = payload.downcast_ref::<Box<String>>() {
            return (**text).clone();
        }
        match payload.downcast::<Box<dyn Any + Send>>() {
            // The downcast's `Ok` retypes this very allocation, so the
            // re-thrown payload is the box it holds -- the dereference.  The
            // box itself would do for one pass and then match its own
            // wrapper type on every later pass, unrolling nothing and
            // looping forever.
            Ok(rethrown) => payload = *rethrown,
            Err(payload) => {
                let inner = &*payload;
                if inner.type_id() == TypeId::of::<()>() {
                    return "#<unit () panic payload -- expect-test reports a \
                             snapshot mismatch this way; the diff between the \
                             expect file and the captured value prints on \
                             stdout, above this report>"
                        .to_owned();
                }
                return format!("#<non-string panic payload {:?}>", inner.type_id());
            }
        }
    }
}

thread_local! {
    /// Where the most recent panic on this thread started, recorded by the
    /// hook [`install_panic_origin_hook`] chains in.
    static PANIC_ORIGIN: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Chain a panic hook that records where each panic starts.
///
/// A caught panic reports its payload, but a `panic_any` payload has no
/// message and a re-thrown one may have lost its origin; the location is
/// the part that survives both. Nextest runs each test in its own process,
/// so the process-wide hook only ever sees this suite's own panics, and
/// the previous hook still prints, so nothing is silenced.
fn install_panic_origin_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(location) = info.location() {
                let origin = format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                );
                PANIC_ORIGIN.with(|cell| *cell.borrow_mut() = Some(origin));
            }
            previous(info);
        }));
    });
}

/// Run `phase`, turning a panic into a `String` labelled with `label`.
///
/// A suite reports every failed step of a scenario rather than dying on the
/// first one, which needs the panic as a value. The report carries where
/// the panic started, which a message-less payload cannot say for itself.
pub fn catch_phase<T>(label: &str, phase: impl FnOnce() -> T) -> Result<T, String> {
    install_panic_origin_hook();
    PANIC_ORIGIN.with(|cell| *cell.borrow_mut() = None);
    catch_unwind(AssertUnwindSafe(phase)).map_err(|payload| {
        let origin = PANIC_ORIGIN.with(|cell| cell.borrow().clone());
        match origin {
            Some(origin) => format!("{label}: {} (panicked at {origin})", panic_text(payload)),
            None => format!("{label}: {}", panic_text(payload)),
        }
    })
}

/// Run one step against both editors, keeping both failures.
///
/// The editors are separate processes on separate PTYs; a step that fails on
/// one of them still has to run on the other, or the two screens drift apart
/// and every later step compares states that were never meant to match.
pub fn both(
    pair: &mut PackageTuiPair,
    label: &str,
    operation: impl Fn(&mut TuiSession) + Copy,
) -> Result<(), String> {
    let gnu = catch_phase(&format!("GNU {label}"), || operation(&mut pair.gnu));
    let neo = catch_phase(&format!("Neo {label}"), || operation(&mut pair.neo));
    let errors = [gnu.err(), neo.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// The single row holding `marker`, trimmed.
pub fn exact_row(session: &TuiSession, marker: &str) -> String {
    session
        .text_grid()
        .into_iter()
        .find(|row| row.contains(marker))
        .unwrap_or_else(|| panic!("{} did not render {marker:?}", session.name))
        .trim()
        .to_owned()
}
