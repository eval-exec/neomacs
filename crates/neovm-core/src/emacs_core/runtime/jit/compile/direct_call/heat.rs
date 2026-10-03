//! Compile-time admission for named self sites using existing source heat.
//!
//! `NEOVM_JIT_DIRECT_SELF_HEAT=seen|hot` can decline the expanded direct
//! site and its implicit register ABI when the compiler has only a cold
//! first-sight callee. `off` preserves the original self policy. The gate
//! adds no recording: `Seen` requires a prior source entry; `Hot` requires
//! the normal JIT tier threshold. Heat is invocation/loop evidence, not a
//! proof that a particular self-call branch ran.
//!
//! A declined source can stay compiled with memory ABI indefinitely: its
//! native spec calls do not advance interpreter heat, and self-recursive
//! bodies already use the full allocator, so legacy allocator re-tiering
//! does not supply a later promotion. Keeping the modes at or below the
//! ordinary dispatch threshold preserves direct admission when a source
//! naturally tiers after interpreted entries.
//!
//! Threading: immutable compiler configuration and scalar observations of
//! the source's existing atomic heat; no new runtime or mutator state.
//! The compiler reads the source once for a build. Concurrent heat changes
//! can alter only optimization admission, never call semantics or ABI
//! publication. Off, the previous self-source scope and CLIF are unchanged.

/// Whether an existing heat observation admits self-direct compilation.
/// Threading: immutable value copied into one compiler's admission decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DirectSelfHeat {
    /// Preserve the original self policy, regardless of heat.
    #[default]
    Off,
    /// Admit once at least one source entry has contributed heat.
    Seen,
    /// Admit once the source reaches the ordinary dispatch threshold.
    Hot,
}

impl DirectSelfHeat {
    /// Missing, disabled and unrecognized settings keep the original policy.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("seen") => Self::Seen,
            Some("hot") => Self::Hot,
            _ => Self::Off,
        }
    }

    /// Whether a compile's existing heat snapshot meets this mode. `Hot`
    /// follows an overridden ordinary threshold, including threshold zero.
    pub(crate) const fn allows(self, heat: u32, threshold: u32) -> bool {
        match self {
            Self::Off => true,
            Self::Seen => heat != 0,
            Self::Hot => heat >= threshold,
        }
    }
}

#[cfg(test)]
#[path = "heat/tests/heat_test.rs"]
mod tests;
