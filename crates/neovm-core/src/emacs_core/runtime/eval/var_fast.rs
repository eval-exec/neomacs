//! Cached variable tiers (P1.4 Stage A): a read, `setq`, `let` or unbind of a
//! buffer-local (GNU `SYMBOL_LOCALIZED`) or forwarded (`SYMBOL_FORWARDED`)
//! variable answered from the caches the general path would consult, without
//! the general path.
//!
//! Each tier is the cache-hit prefix of one existing general path, named in
//! its doc comment. It mirrors that path exactly for the shapes it accepts
//! and refuses everything else -- `None` or `false`, having changed nothing
//! -- so a refusal runs the unchanged general path with the original
//! operands. No tier runs Lisp, signals, allocates a Lisp object or reaches a
//! safe point, so the caller needs no GC root for the value, and the general
//! path's quit bracket, watchers, constant check, type signals and
//! `debug-on-exit` are either impossible on a hit or refused into it.
//!
//! # Knob
//!
//! `NEOVM_VAR_CACHE` selects the tiers, for a same-binary A/B: unset, `1`,
//! `on` or `all` enables all four; `0`, `off` or `none` disables them; a
//! comma list of `read`, `set`, `bind`, `unbind` enables just those. Read
//! once per process, on the first variable op that reaches a tier.
//!
//! # Census
//!
//! Under `--features vm-profile` (and in tests) every tier counts its hits
//! per shape and its refusals ([`VarCacheEvent`]); the `VAR-CACHE` section of
//! `neovm--vm-profile-dump` prints them.

use super::*;
use crate::emacs_core::forward::{ForwardStore, LispBufferObjFwd, LispFwd, LispFwdType};
use crate::emacs_core::symbol::{
    BlvCacheHit, LispSymbol, SYMCELL_INLINE_WRITE_MASK, SymbolRedirect, symcell_inline_write_value,
};
use std::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Knob
// ---------------------------------------------------------------------------

/// One cached tier, as `NEOVM_VAR_CACHE` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum VarCacheTier {
    /// [`Context::read_var_cached`] (`Bvarref`).
    Read = 1 << 0,
    /// The cached `setq` (`Bvarset`).
    Set = 1 << 1,
    /// The cached `specbind` (`Bvarbind`, `let`).
    Bind = 1 << 2,
    /// The cached `do_one_unbind` arms (`Bunbind`, every `unbind_to`).
    Unbind = 1 << 3,
}

impl VarCacheTier {
    pub(crate) const ALL: [Self; 4] = [Self::Read, Self::Set, Self::Bind, Self::Unbind];

    #[inline(always)]
    const fn bit(self) -> u8 {
        self as u8
    }

    fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Set => "set",
            Self::Bind => "bind",
            Self::Unbind => "unbind",
        }
    }
}

/// Every tier's bit.
const ALL_TIERS: u8 = VarCacheTier::Read.bit()
    | VarCacheTier::Set.bit()
    | VarCacheTier::Bind.bit()
    | VarCacheTier::Unbind.bit();
/// [`VAR_CACHE_TIERS`] before the knob is read.
const TIERS_UNREAD: u8 = 0x80;
const _: () = assert!(ALL_TIERS & TIERS_UNREAD == 0);

/// The enabled tiers' bits, or [`TIERS_UNREAD`].
static VAR_CACHE_TIERS: AtomicU8 = AtomicU8::new(TIERS_UNREAD);

/// The tier set a value of `NEOVM_VAR_CACHE` selects (see the module docs).
pub(crate) fn parse_var_cache_knob(value: Option<&str>) -> u8 {
    let Some(value) = value.map(str::trim) else {
        return ALL_TIERS;
    };
    match value.to_ascii_lowercase().as_str() {
        "" | "1" | "on" | "true" | "yes" | "all" => ALL_TIERS,
        "0" | "off" | "false" | "no" | "none" => 0,
        list => list
            .split(',')
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .fold(0, |tiers, word| {
                match VarCacheTier::ALL.iter().find(|tier| tier.name() == word) {
                    Some(tier) => tiers | tier.bit(),
                    None => {
                        tracing::warn!(word, "NEOVM_VAR_CACHE: unknown tier ignored");
                        tiers
                    }
                }
            }),
    }
}

#[cfg(test)]
thread_local! {
    static TIERS_TEST_OVERRIDE: Cell<Option<u8>> = const { Cell::new(None) };
}

/// Enable exactly TIERS on this thread, overriding the knob (tests only).
#[cfg(test)]
pub(crate) fn set_var_cache_tiers_for_test(tiers: &[VarCacheTier]) {
    let bits = tiers.iter().fold(0, |bits, tier| bits | tier.bit());
    TIERS_TEST_OVERRIDE.with(|c| c.set(Some(bits)));
}

/// Whether TIER is enabled. One relaxed byte load once the knob is read.
#[inline(always)]
pub(crate) fn var_cache_tier_on(tier: VarCacheTier) -> bool {
    #[cfg(test)]
    if let Some(bits) = TIERS_TEST_OVERRIDE.with(|c| c.get()) {
        return bits & tier.bit() != 0;
    }
    let tiers = VAR_CACHE_TIERS.load(Ordering::Relaxed);
    if tiers & TIERS_UNREAD != 0 {
        return read_var_cache_knob() & tier.bit() != 0;
    }
    tiers & tier.bit() != 0
}

#[cold]
#[inline(never)]
fn read_var_cache_knob() -> u8 {
    let tiers = parse_var_cache_knob(std::env::var("NEOVM_VAR_CACHE").ok().as_deref());
    VAR_CACHE_TIERS.store(tiers, Ordering::Relaxed);
    tiers
}

// ---------------------------------------------------------------------------
// Census
// ---------------------------------------------------------------------------

/// What one tier did with one variable op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum VarCacheEvent {
    /// Read a buffer-local variable from its BLV cache.
    ReadLocalized,
    /// Read a forwarder that holds its own value (Obj, Bool, Int, Kboard).
    ReadForwarded,
    /// Read a per-buffer slot of the current buffer.
    ReadBufferSlot,
    /// A buffer-local or forwarded read the tier left to the general path.
    ReadRefused,
    /// `setq` of a buffer-local variable into the buffer's own binding.
    SetLocalizedFound,
    /// `setq` of a buffer-local variable (not `local_if_set`) with no
    /// binding here: the default.
    SetLocalizedDefault,
    /// `setq` of a forwarder that holds its own value.
    SetForwarded,
    /// A buffer-local or forwarded `setq` the tier left to the general path.
    SetRefused,
    /// `let` of a buffer-local variable with a binding here (`LetLocal`).
    BindLetLocal,
    /// `let` of a buffer-local variable's default (`LetDefault`).
    BindLetDefault,
    /// `let` of a forwarder that holds its own value (`Let`).
    BindForwarded,
    /// A buffer-local or forwarded `let` the tier left to the general path.
    BindRefused,
}

#[cfg(any(test, feature = "vm-profile"))]
impl VarCacheEvent {
    pub(crate) const ALL: [Self; 12] = [
        Self::ReadLocalized,
        Self::ReadForwarded,
        Self::ReadBufferSlot,
        Self::ReadRefused,
        Self::SetLocalizedFound,
        Self::SetLocalizedDefault,
        Self::SetForwarded,
        Self::SetRefused,
        Self::BindLetLocal,
        Self::BindLetDefault,
        Self::BindForwarded,
        Self::BindRefused,
    ];
    const COUNT: usize = Self::ALL.len();

    fn name(self) -> &'static str {
        match self {
            Self::ReadLocalized => "read   localized (BLV hit)",
            Self::ReadForwarded => "read   forwarded (Obj/Bool/Int/Kboard)",
            Self::ReadBufferSlot => "read   per-buffer slot",
            Self::ReadRefused => "read   refused -> general path",
            Self::SetLocalizedFound => "setq   localized, own binding",
            Self::SetLocalizedDefault => "setq   localized, default",
            Self::SetForwarded => "setq   forwarded (Obj/Bool/Int/Kboard)",
            Self::SetRefused => "setq   refused -> general path",
            Self::BindLetLocal => "let    localized, own binding (LetLocal)",
            Self::BindLetDefault => "let    localized, default (LetDefault)",
            Self::BindForwarded => "let    forwarded (Obj/Bool/Int)",
            Self::BindRefused => "let    refused -> general path",
        }
    }
}

#[cfg(any(test, feature = "vm-profile"))]
thread_local! {
    static VAR_CACHE_EVENTS: [Cell<u64>; VarCacheEvent::COUNT] =
        const { [const { Cell::new(0) }; VarCacheEvent::COUNT] };
}

/// Count EVENT (a no-op unless testing or profiling).
#[inline(always)]
fn note(event: VarCacheEvent) {
    #[cfg(any(test, feature = "vm-profile"))]
    VAR_CACHE_EVENTS.with(|events| {
        let cell = &events[event as usize];
        cell.set(cell.get() + 1);
    });
    #[cfg(not(any(test, feature = "vm-profile")))]
    let _ = event;
}

/// How many times EVENT happened on this thread since the last reset.
#[cfg(any(test, feature = "vm-profile"))]
pub(crate) fn var_cache_event_count(event: VarCacheEvent) -> u64 {
    VAR_CACHE_EVENTS.with(|events| events[event as usize].get())
}

/// Forget every counted event on this thread.
#[cfg(any(test, feature = "vm-profile"))]
pub(crate) fn reset_var_cache_events() {
    VAR_CACHE_EVENTS.with(|events| events.iter().for_each(|cell| cell.set(0)));
}

/// The `VAR-CACHE` section of the VM profile dump: every event's count.
#[cfg(any(test, feature = "vm-profile"))]
pub(crate) fn var_cache_census_report(label: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let enabled = VarCacheTier::ALL
        .iter()
        .filter(|tier| var_cache_tier_on(**tier))
        .map(|tier| tier.name())
        .collect::<Vec<_>>()
        .join(",");
    let _ = writeln!(
        out,
        "=== VAR-CACHE [{label}]: cached variable tiers (enabled: {}) ===",
        if enabled.is_empty() { "none" } else { &enabled }
    );
    for event in VarCacheEvent::ALL {
        let _ = writeln!(
            out,
            "  {:<44} {:>12}",
            event.name(),
            var_cache_event_count(event)
        );
    }
    out
}

// ---------------------------------------------------------------------------
// The tiers
// ---------------------------------------------------------------------------

impl Context {
    /// GNU `Bvarref` of a buffer-local or forwarded variable when the answer
    /// needs neither a swap-in nor a `Vm`: the cache-hit prefix of
    /// `Vm::fast_path_var_ref` + `Vm::lookup_var_id`.
    ///
    /// - A buffer-local variable whose BLV cache is loaded for the current
    ///   buffer at the current epoch reads the loaded cell's cdr (GNU
    ///   `swap_in_symval_forwarding`'s early-out; `lookup_var_id`'s first
    ///   `Localized` arm, `read_localized_symbol_for_buffer`).
    /// - A forwarder that holds its own value reads it (`LispFwd::load`, the
    ///   `fast_path_var_ref` arm).
    /// - A per-buffer slot reads the current buffer's slot, or the default
    ///   (`LispBufferObjFwd::value_in`, the `find_symbol_value_in_buffer`
    ///   arm).
    ///
    /// `None` for anything else -- a plain or aliased symbol, a BLV miss,
    /// no current buffer, a void value (the general path signals
    /// `void-variable`) -- and when the `read` tier is off.
    #[inline(never)]
    pub(crate) fn read_var_cached(&self, id: SymId) -> Option<Value> {
        if !var_cache_tier_on(VarCacheTier::Read) {
            return None;
        }
        let sym = self.obarray.get_by_id(id)?;
        let value = match sym.redirect() {
            SymbolRedirect::Localized => self.read_localized_cached(sym),
            SymbolRedirect::Forwarded => self.read_forwarded_cached(sym),
            SymbolRedirect::Plainval | SymbolRedirect::Varalias => return None,
        };
        if value.is_none() {
            note(VarCacheEvent::ReadRefused);
        }
        value
    }

    #[inline(always)]
    fn read_localized_cached(&self, sym: &LispSymbol) -> Option<Value> {
        let buf = self.buffers.current_buffer()?;
        let hit = sym.blv_cache_hit(buf.id)?;
        let value = hit.valcell.cons_cdr();
        if value.is_unbound() {
            return None;
        }
        note(VarCacheEvent::ReadLocalized);
        Some(value)
    }

    #[inline(always)]
    fn read_forwarded_cached(&self, sym: &LispSymbol) -> Option<Value> {
        let fwd = sym.forwarded_descriptor()?;
        if let Some(value) = fwd.load() {
            note(VarCacheEvent::ReadForwarded);
            return Some(value);
        }
        // `load` answers `None` for exactly the per-buffer slot.
        debug_assert_eq!(fwd.ty, LispFwdType::BufferObj);
        let buf = self.buffers.current_buffer()?;
        // SAFETY: a `BufferObj` descriptor is a `LispBufferObjFwd`, whose
        // first field is the shared header (`#[repr(C)]`).
        let buf_fwd = unsafe { &*(fwd as *const LispFwd as *const LispBufferObjFwd) };
        let value = buf_fwd.value_in(
            Some(&buf.slots[..]),
            buf.local_flags,
            Some(&self.buffers.buffer_defaults[..]),
        );
        if value.is_unbound() {
            return None;
        }
        note(VarCacheEvent::ReadBufferSlot);
        Some(value)
    }
    /// GNU `set_internal (sym, val, Qnil, SET)` -- bytecode `Bvarset` -- of
    /// a buffer-local or forwarded variable when the store needs no swap-in,
    /// no watcher, no `let_shadows` walk, no allocation and no
    /// republication: the cache-hit prefix of `Vm::assign_var_id`.
    ///
    /// The symbol must be untrapped (no watcher, not a constant), not
    /// host-projected (flag bit and `runtime_binding_has_projection`) and an
    /// interned member: [`SYMCELL_INLINE_WRITE_MASK`] over its write window,
    /// the one test the inline JIT stores will use. Then:
    /// - buffer-local, BLV loaded for the current buffer at the current
    ///   epoch, and either the buffer's own binding (`found`) or no binding
    ///   and not `local_if_set` (the default, with `valcell == defcell`):
    ///   the loaded cell's cdr takes the value through the BLV forwarder's
    ///   type rule. That is `set_internal_localized_with`'s whole effect on
    ///   such a hit: it re-selects the same cell and rewrites `where`,
    ///   `alist_epoch` and `found` with the values they hold, and the alist
    ///   comes back unchanged.
    /// - forwarded Obj/Bool/Int/Kboard (not a per-buffer slot): the
    ///   descriptor's typed store, exactly `set_symbol_value_id`'s.
    ///
    /// `false`, having stored nothing, for anything else -- a plain cell
    /// (`try_set_plain_variable` owns it), an alias, a BLV miss, an
    /// auto-creating `local_if_set` store, a per-buffer slot, a value the
    /// type rule refuses (the general path signals) -- and when the `set`
    /// tier is off.
    #[inline(never)]
    pub(crate) fn try_set_var_cached(&mut self, id: SymId, value: Value) -> bool {
        if !var_cache_tier_on(VarCacheTier::Set) {
            return false;
        }
        let Some(sym) = self.obarray.get_by_id(id) else {
            return false;
        };
        let window = sym.write_window() & SYMCELL_INLINE_WRITE_MASK;
        let stored = if window == symcell_inline_write_value(SymbolRedirect::Localized) {
            self.set_localized_cached(sym, id, value)
        } else if window == symcell_inline_write_value(SymbolRedirect::Forwarded) {
            self.set_forwarded_cached(sym, id, value)
        } else {
            return false;
        };
        if !stored {
            note(VarCacheEvent::SetRefused);
        }
        stored
    }

    #[inline(always)]
    fn set_localized_cached(&self, sym: &LispSymbol, id: SymId, value: Value) -> bool {
        // `assign_var_id` publishes the write to the host projections.
        if self.runtime_binding_has_projection(id) {
            return false;
        }
        // Its Localized arm needs a current buffer.
        let Some(buf) = self.buffers.current_buffer() else {
            return false;
        };
        let Some(hit) = sym.blv_cache_hit(buf.id) else {
            return false;
        };
        let Some(stored) = forward_rule(hit.fwd, value) else {
            return false;
        };
        let (cell, event) = if hit.found {
            (hit.valcell, VarCacheEvent::SetLocalizedFound)
        } else if !hit.local_if_set && hit.valcell.bits() == hit.defcell.bits() {
            (hit.defcell, VarCacheEvent::SetLocalizedDefault)
        } else {
            // `local_if_set` with no binding here: auto-create unless a
            // `let` shadows the buffer -- a specpdl walk and a cons.
            return false;
        };
        cell.set_cdr(stored);
        note(event);
        true
    }

    #[inline(always)]
    fn set_forwarded_cached(&self, sym: &LispSymbol, id: SymId, value: Value) -> bool {
        // `assign_var_id` republishes (and marks redisplay for) these.
        if self.runtime_binding_has_projection(id) {
            return false;
        }
        let Some(fwd) = sym.forwarded_descriptor() else {
            return false;
        };
        // A per-buffer slot has local-flag and default-propagation rules;
        // `store_runtime_binding` writes a slot-named symbol's buffer slot.
        if fwd.ty == LispFwdType::BufferObj
            || crate::buffer::buffer::lookup_buffer_slot_by_sym_id(id).is_some()
        {
            return false;
        }
        let Ok(store) = fwd.store(value) else {
            return false;
        };
        fwd.commit(store);
        note(VarCacheEvent::SetForwarded);
        true
    }

    /// GNU `specbind` -- bytecode `Bvarbind`, `let` -- of a buffer-local or
    /// forwarded variable when the bind needs no swap-in, no watcher and no
    /// type signal: the cache-hit prefix of `specbind_resolved`'s `Localized`
    /// arm and its plain/forwarded tail.
    ///
    /// The symbol must pass [`SYMCELL_INLINE_WRITE_MASK`] (untrapped, not
    /// flag-projected, interned). A bind republishes nothing but the
    /// flag-projected mirrors (`sync_cached_runtime_binding_by_id`), so the
    /// projection mask does not apply. Then:
    /// - buffer-local, BLV loaded for the current buffer at the current
    ///   epoch: record `LetLocal` (the buffer's own binding) or `LetDefault`
    ///   (no binding, `valcell == defcell`) with the loaded cell's value, and
    ///   store the value, through the BLV forwarder's type rule, into that
    ///   cell -- what `find_symbol_value_in_buffer`,
    ///   `has_per_buffer_binding` and `set_internal_localized (BIND)` do on
    ///   such a hit (a bind never auto-creates).
    /// - forwarded Obj/Bool/Int: record `Let` with the descriptor's value
    ///   and take its typed store (GNU `SPECPDL_LET`, `do_specbind`).
    ///
    /// `false`, with nothing pushed or stored, for anything else -- a plain
    /// cell, an alias, a per-buffer slot, a keyboard variable, a BLV miss,
    /// a void old value, a value the type rule refuses (the general path
    /// signals after its push) -- and when the `bind` tier is off.
    #[inline(never)]
    pub(crate) fn specbind_cached(&mut self, id: SymId, value: Value) -> bool {
        if !var_cache_tier_on(VarCacheTier::Bind) {
            return false;
        }
        let Some(sym) = self.obarray.get_by_id(id) else {
            return false;
        };
        let window = sym.write_window() & SYMCELL_INLINE_WRITE_MASK;
        // Everything the bind needs from the symbol is copied out here, so
        // its borrow ends before the push.
        let bound = if window == symcell_inline_write_value(SymbolRedirect::Localized) {
            match self.buffers.current_buffer().map(|buf| buf.id) {
                Some(buffer_id) => match sym.blv_cache_hit(buffer_id) {
                    Some(hit) => self.specbind_localized_hit(id, buffer_id, hit, value),
                    None => false,
                },
                None => false,
            }
        } else if window == symcell_inline_write_value(SymbolRedirect::Forwarded) {
            match sym.forwarded_descriptor() {
                Some(fwd) => self.specbind_forwarded_cached(id, fwd, value),
                None => false,
            }
        } else {
            return false;
        };
        if !bound {
            note(VarCacheEvent::BindRefused);
        }
        bound
    }

    #[inline(always)]
    fn specbind_localized_hit(
        &mut self,
        id: SymId,
        buffer_id: crate::buffer::BufferId,
        hit: BlvCacheHit,
        value: Value,
    ) -> bool {
        let old = hit.valcell.cons_cdr();
        if old.is_unbound() {
            return false;
        }
        let Some(stored) = forward_rule(hit.fwd, value) else {
            return false;
        };
        if hit.found {
            self.push_specpdl_with(|| SpecBinding::LetLocal {
                sym_id: id,
                old_value: old,
                buffer_id,
            });
            note(VarCacheEvent::BindLetLocal);
        } else if hit.valcell.bits() == hit.defcell.bits() {
            self.push_specpdl_with(|| SpecBinding::LetDefault {
                sym_id: id,
                old_value: SavedBindingValue::from_option(Some(old)),
                buffer_id: SavedBufferId::from_option(Some(buffer_id)),
            });
            note(VarCacheEvent::BindLetDefault);
        } else {
            return false;
        }
        hit.valcell.set_cdr(stored);
        true
    }

    #[inline(always)]
    fn specbind_forwarded_cached(
        &mut self,
        id: SymId,
        fwd: &'static LispFwd,
        value: Value,
    ) -> bool {
        // A per-buffer slot binds `LetLocal`/`LetDefault` by its local flag;
        // a keyboard variable is GNU's `where.kbd` binding.
        if matches!(fwd.ty, LispFwdType::BufferObj | LispFwdType::KboardObj) {
            return false;
        }
        let Some(old) = fwd.load().filter(|old| !old.is_unbound()) else {
            return false;
        };
        let Ok(store) = fwd.store(value) else {
            return false;
        };
        self.push_specpdl_with(|| SpecBinding::Let {
            sym_id: id,
            old_value: SavedBindingValue::from_option(Some(old)),
        });
        fwd.commit(store);
        note(VarCacheEvent::BindForwarded);
        true
    }
}

/// `store_symval_forwarding`'s type rule for a store governed by FWD, as
/// `check_forwarded_store` applies it: the value to store (a Boolean slot
/// canonicalises to `t`/`nil`), or `None` where the general path signals or
/// where FWD is a per-buffer slot, whose rule depends on the buffer.
#[inline(always)]
fn forward_rule(fwd: Option<&'static LispFwd>, value: Value) -> Option<Value> {
    match fwd {
        None => Some(value),
        Some(fwd) if fwd.ty == LispFwdType::BufferObj => None,
        Some(fwd) => fwd.store(value).ok().map(ForwardStore::canonical_value),
    }
}
