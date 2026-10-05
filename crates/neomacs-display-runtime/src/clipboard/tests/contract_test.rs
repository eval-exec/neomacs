//! The contract every [`ClipboardBackend`] satisfies, and the fakes that prove
//! the assertions have teeth.
//!
//! The contract is deliberately about the *shape* of an answer, not about any
//! one backend's mechanism:
//!
//! 1. an absent selection is an answer (`TextRead::NoSelection`,
//!    `TextRead::TargetUnavailable`, or `TextRead::Indeterminate`), never an
//!    `Err` — `Err` is reserved for transport and worker failures;
//! 2. text round-trips where the backend can read the selection it set;
//! 3. `Text("")` is text, not absence;
//! 4. CLIPBOARD and PRIMARY never alias.
//!
//! What these tests cannot see: the real X11 owner/requestor exchange, the
//! Wayland data-control protocol against a compositor, and cross-process
//! ownership.  The GUI test job covers PRIMARY end to end; the live check at
//! the bottom is opt-in.

use super::*;

/// The smallest backend that honours the contract: one owned string per
/// selection.
#[derive(Default)]
struct FakeBackend {
    selections: std::collections::HashMap<ClipboardSelection, String>,
}

impl ClipboardBackend for FakeBackend {
    fn set_text(
        &mut self,
        selection: ClipboardSelection,
        text: Option<&str>,
    ) -> Result<(), String> {
        match text {
            Some(text) => {
                self.selections.insert(selection, text.to_owned());
            }
            None => {
                self.selections.remove(&selection);
            }
        }
        Ok(())
    }

    fn text(&mut self, selection: ClipboardSelection) -> Result<text_policy::TextRead, String> {
        Ok(match self.selections.get(&selection) {
            Some(text) => text_policy::TextRead::Text(text.clone()),
            None => text_policy::TextRead::NoSelection,
        })
    }

    fn owner(&mut self, selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(if self.selections.contains_key(&selection) {
            SelectionOwner::ThisProcess
        } else {
            SelectionOwner::None
        })
    }
}

/// Reports every absence as a transport error; the contract's first check
/// must catch this.
struct LyingBackend;

impl ClipboardBackend for LyingBackend {
    fn set_text(
        &mut self,
        _selection: ClipboardSelection,
        _text: Option<&str>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn text(&mut self, _selection: ClipboardSelection) -> Result<text_policy::TextRead, String> {
        Err("selection is empty".to_owned())
    }

    fn owner(&mut self, _selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::Unknown)
    }
}

/// Answers both selections from one slot; the contract's alias check must
/// catch this.
#[derive(Default)]
struct AliasingBackend {
    value: String,
}

impl ClipboardBackend for AliasingBackend {
    fn set_text(
        &mut self,
        _selection: ClipboardSelection,
        text: Option<&str>,
    ) -> Result<(), String> {
        self.value = text.unwrap_or_default().to_owned();
        Ok(())
    }

    fn text(&mut self, _selection: ClipboardSelection) -> Result<text_policy::TextRead, String> {
        Ok(text_policy::TextRead::Text(self.value.clone()))
    }

    fn owner(&mut self, _selection: ClipboardSelection) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::Unknown)
    }
}

/// Assert the text contract against any backend.
fn assert_contract(backend: &mut dyn ClipboardBackend) {
    const PROBE: &str = "neomacs clipboard contract probe 3f8a2c";
    let clipboard = ClipboardSelection::Clipboard;
    let primary = ClipboardSelection::Primary;

    // 1. Absence is an answer, not a transport error.
    backend
        .text(clipboard)
        .expect("absence must be an answer, not a transport error");

    // 2. Text round-trips.
    backend
        .set_text(clipboard, Some(PROBE))
        .expect("setting text must not fail");
    assert_eq!(
        backend.text(clipboard).expect("reading text must not fail"),
        text_policy::TextRead::Text(PROBE.to_owned()),
        "a backend must read back the text it was given"
    );

    // 3. An empty string is text, not absence.
    backend
        .set_text(clipboard, Some(""))
        .expect("setting empty text must not fail");
    assert_eq!(
        backend.text(clipboard).expect("reading text must not fail"),
        text_policy::TextRead::Text(String::new()),
        "an empty selection is text, not absence"
    );

    // 4. CLIPBOARD and PRIMARY never alias.
    backend
        .set_text(clipboard, Some(PROBE))
        .expect("setting text must not fail");
    assert_ne!(
        backend
            .text(primary)
            .expect("reading PRIMARY must not fail"),
        text_policy::TextRead::Text(PROBE.to_owned()),
        "PRIMARY must not alias CLIPBOARD"
    );

    // 1 again, for the cleared state.
    backend
        .set_text(clipboard, None)
        .expect("clearing must not fail");
    let cleared = backend
        .text(clipboard)
        .expect("absence must be an answer, not a transport error");
    assert_ne!(
        cleared,
        text_policy::TextRead::Text(String::new()),
        "a cleared selection must not read as empty text"
    );
}

#[test]
fn the_fake_backend_satisfies_the_text_contract() {
    assert_contract(&mut FakeBackend::default());
}

#[test]
#[should_panic(expected = "absence must be an answer")]
fn a_backend_that_reports_absence_as_an_error_fails_the_contract() {
    assert_contract(&mut LyingBackend);
}

#[test]
#[should_panic(expected = "PRIMARY must not alias CLIPBOARD")]
fn a_backend_that_aliases_the_selections_fails_the_contract() {
    assert_contract(&mut AliasingBackend::default());
}

/// Opt-in live check against the real system clipboard.  It writes and clears
/// the CLIPBOARD selection, so it runs only when
/// `NEOMACS_CLIPBOARD_LIVE_TEST` is set.
#[test]
fn live_backend_satisfies_the_text_contract() {
    if std::env::var_os("NEOMACS_CLIPBOARD_LIVE_TEST").is_none() {
        return;
    }
    let mut backend = ArboardClipboard::new().expect("system clipboard should open");
    assert_contract(&mut backend);
}
