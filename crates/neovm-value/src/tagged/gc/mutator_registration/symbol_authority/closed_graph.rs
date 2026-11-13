//! Test executor admission only. No running raw-value API is added.
//!
//! These are C4 read-only predicates over the stopped keeper: the two
//! `KeeperSymbols` methods and their graph walk add no write or GC route.
//! They live here (not in core_tests) because an inherent `impl KeeperSymbols`
//! must be defined in this crate; the test-support build publishes them for
//! the re-homed core tests, while production keeps the module absent.
use super::*;
use crate::value::Value;

/// All words come from live, rooted sources on this stopped heap. Admit only
/// bare symbols, fixnums, nil/t and cons graphs; cycles are visited once.
/// Other payloads would need a separate sharing proof, even inside a cons.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn closed_graph(roots: impl IntoIterator<Item = Value>, budget: usize) -> bool {
    let mut pending: Vec<_> = roots.into_iter().map(|word| (word, true)).collect();
    let mut seen = rustc_hash::FxHashSet::default();
    let mut visited = 0usize;
    while let Some((word, root)) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > budget {
            return false;
        }
        if word.is_unbound() {
            if root {
                continue;
            }
            return false;
        }
        if word.is_nil() || word.is_t() || word.is_fixnum() || word.is_bare_symbol() {
            continue;
        }
        if !word.is_cons() {
            return false;
        }
        if seen.insert(word.bits()) {
            // Pure copies under stop-all, from a rooted live World graph.
            // No observation/TLS path and no mutable reference is issued.
            pending.push((word.cons_car_unobserved(), false));
            pending.push((word.cons_cdr_unobserved(), false));
        }
    }
    true
}

impl KeeperSymbols<'_> {
    /// C5's separate frozen input admission. Do not broaden the C3 graph
    /// closure: these sessions import only exact string roots, never their
    /// properties or mutable payload graphs as executor operands.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn initial_names_closed_for_test(
        &self,
        keys: &[SymbolHandle],
        budget: usize,
    ) -> Result<bool, RegistrationError> {
        if keys.len() > budget {
            return Ok(false);
        }
        for key in keys {
            self.backing(key)?;
            // SAFETY: matching stopped heap/World, retained rooted input,
            // and zero symbol word checked by the keeper admission.
            let bits = unsafe { self.registry.symbols.backing.probe(key.symbol) };
            // SAFETY: `probe` returned a word from this keeper's admitted Plain
            // backing under stop-all; it is inspected immediately and not kept.
            let value = unsafe { Value::from_bits(bits) };
            let Some(name) = value.as_lisp_string() else {
                return Ok(false);
            };
            if name.as_bytes().first() == Some(&b':') {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn plain_graphs_closed_for_test(
        &self,
        keys: &[SymbolHandle],
        budget: usize,
    ) -> Result<bool, RegistrationError> {
        let mut roots = Vec::with_capacity(keys.len());
        for key in keys {
            self.backing(key)?;
            // SAFETY: backing() validated this exact heap/World/era/key and
            // the keeper holds stop-all with a zero word. probe is test-only.
            let bits = unsafe { self.registry.symbols.backing.probe(key.symbol) };
            // SAFETY: same admitted stopped backing as above; the word is
            // copied into the temporary closure roots only for this walk.
            roots.push(unsafe { Value::from_bits(bits) });
        }
        Ok(closed_graph(roots, budget))
    }
}
