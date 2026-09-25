//! The per-leaf MIR verdict: why the MIR tier did or did not take the body
//! of one compile, kept on the leaf that compile produced.
//!
//! The bail census (`[neovm-jit-final-mir-bails]`) is aggregate only, so the
//! cycles of a baseline leaf could not be tied to the reason it is not a MIR
//! leaf without isolating functions by hand. Each compile now records its
//! verdict, and the exit report prints it per leaf (`mir=` on
//! `[neovm-jit-final-leaf]`) and per compile attempt (`#mir` rows in
//! `NEOVM_JIT_PROFILE`).
//!
//! A verdict is one of:
//! * `taken`: the MIR tier produced the leaf;
//! * the pre-build gates that kept MIR from being built, in the funnel's
//!   names and joined by `+` (`gate_opt+gate_prefix`);
//! * the first bail key the compile recorded, exactly as the census counts
//!   it (`build:...`, `gate:...`, `lower:...`, or an adapter key such as
//!   `opaque-bind:SaveExcursion`, which a failed lowering records before its
//!   generic `lower:` key);
//! * nothing (`-`): the compile never reached the MIR tier (for example
//!   dynamically bound parameters), or it is an OSR or AOT leaf.
//!
//! Recorded only while a report knob is set (`super::report_requested`):
//! with every knob off none of this runs.

use std::cell::RefCell;

/// One compile's verdict, built from the funnel events in the order the
/// compile raises them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MirVerdict(Option<String>);

impl MirVerdict {
    /// A pre-build gate tripped. Gates are counted independently (a body
    /// can trip several), so every one is kept.
    pub(crate) fn gate(&mut self, name: &str) {
        match &mut self.0 {
            None => self.0 = Some(name.to_string()),
            Some(v) if v.starts_with("gate_") => {
                v.push('+');
                v.push_str(name);
            }
            Some(_) => {}
        }
    }

    /// A bail key. The first one names the verdict.
    pub(crate) fn bail(&mut self, key: &str) {
        if self.0.is_none() {
            self.0 = Some(key.to_string());
        }
    }

    /// The MIR tier took the body.
    pub(crate) fn taken(&mut self) {
        self.0 = Some("taken".to_string());
    }

    /// The verdict as one report token: whitespace, commas and `=` become
    /// `_`, so it is a single field of a `key=value` line and a single CSV
    /// column.
    pub(crate) fn render(&self) -> Option<Box<str>> {
        self.0
            .as_deref()
            .map(|v| super::epoch::report_token(v).into_boxed_str())
    }
}

thread_local! {
    /// The verdict of the compile in progress on this thread.
    static CURRENT: RefCell<MirVerdict> = RefCell::default();
}

/// Start a compile's verdict (a compile never nests another).
pub(crate) fn begin() {
    if super::report_requested() {
        CURRENT.with(|v| *v.borrow_mut() = MirVerdict::default());
    }
}

/// Apply one funnel event to the verdict in progress.
pub(super) fn note(event: impl FnOnce(&mut MirVerdict)) {
    if super::report_requested() {
        CURRENT.with(|v| event(&mut v.borrow_mut()));
    }
}

/// The finished verdict of the compile that [`begin`] started, rendered.
pub(crate) fn take() -> Option<Box<str>> {
    if !super::report_requested() {
        return None;
    }
    CURRENT.with(|v| std::mem::take(&mut *v.borrow_mut()).render())
}

/// The `NEOVM_JIT_PROFILE` row of one compile attempt's verdict,
/// `#mir,<compiled_id>,<verdict>`, written right after its compile row and
/// joinable with it and with the `#leaf` rows on `compiled_id`. Three
/// columns: the census reader (which keeps rows with at least 13) skips it.
pub(crate) fn profile_row(compiled_id: &str, verdict: Option<&str>) -> String {
    format!("#mir,{compiled_id},{}\n", verdict.unwrap_or("-"))
}
