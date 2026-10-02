//! Operation-owned emacs-mule encoding views.
//!
//! GNU prepares a charset list once and walks it for each character
//! (`encode_coding_emacs_mule`, coding.c). The legacy path rebuilt and sorted
//! the complete registry for every non-ASCII character.

use super::*;

/// A scalar encoder resolved for one conversion. Each mutator owns its view;
/// its map data is immutable and may be shared through Arc. It holds no Lisp
/// values and adds no process or thread-local registry cache.
///
/// The fast case is deliberately narrow: an ordinary Map charset has precisely
/// one reverse lookup. Offset, Subset, Superset and unified charsets retain the
/// registry implementation, including unified-map precedence. The conversion
/// must run no Lisp callbacks while this view is held, so its owning mutator's
/// registry cannot be redefined between preparation and the fallback lookup.
/// Independent mutators use their own existing runtime registries and views.
pub(crate) struct CharsetEncoder {
    kind: CharsetEncoderKind,
}

enum CharsetEncoderKind {
    Map(Option<Arc<CharsetMapData>>),
    Registry(SymId),
}

impl CharsetEncoder {
    pub(crate) fn new(name: SymId) -> Self {
        let kind = CHARSET_REGISTRY.with(|slot| {
            let registry = slot.borrow();
            let name = registry.resolve_name(name);
            let Some(info) = registry.charsets.get(&name) else {
                return CharsetEncoderKind::Registry(name);
            };
            match &info.method {
                CharsetMethod::Map(map_name) if !info.unified_p => {
                    CharsetEncoderKind::Map(load_charset_map(map_name, info))
                }
                _ => CharsetEncoderKind::Registry(name),
            }
        });
        Self { kind }
    }

    #[inline]
    pub(crate) fn encode_char(&self, ch: i64) -> Option<i64> {
        match &self.kind {
            CharsetEncoderKind::Map(map) => map.as_ref()?.char_to_code.get(&ch).copied(),
            CharsetEncoderKind::Registry(name) => charset_encode_char(*name, ch),
        }
    }
}

struct MuleCandidate {
    name: SymId,
    id: i64,
    dimension: i64,
    encoder: Option<CharsetEncoder>,
}

/// The current Mule charset list, privately owned by one conversion's mutator.
/// This view contains symbol ids and numeric metadata, never GC-managed Values.
/// Candidate maps are resolved only when reached, preserving lazy map loading.
/// No Lisp runs inside the conversion; the next call takes a new view and sees
/// charset definitions, redefinitions, aliases and priority changes. No new
/// shared mutable structure or TLS cache is introduced for concurrent mutators.
pub(crate) struct EmacsMuleEncoder {
    candidates: Vec<MuleCandidate>,
}

impl EmacsMuleEncoder {
    pub(crate) fn new() -> Self {
        let candidates = CHARSET_REGISTRY.with(|slot| {
            let registry = slot.borrow();
            let mut seen = HashSet::new();
            let mut candidates = Vec::new();
            for &name in &registry.emacs_mule_order {
                let name = registry.resolve_name(name);
                if !seen.insert(name) {
                    continue;
                }
                let Some(info) = registry.charsets.get(&name) else {
                    continue;
                };
                if let Some(id) = info.emacs_mule_id {
                    candidates.push(MuleCandidate {
                        name,
                        id,
                        dimension: info.dimension,
                        encoder: None,
                    });
                }
            }

            // Bare Contexts preseed latin-iso8859-1 before mule-conf.el defines
            // it and places it in the ordered list. Preserve that bootstrap
            // support only for entries that have not had a Lisp definition.
            // GNU does not add Mule-list membership when a previously defined
            // non-Mule charset is redefined with a Mule id. A loaded runtime
            // has every real Mule charset ordered;
            // materialized pdump aliases resolve to already-seen canonical
            // entries and therefore cannot duplicate or reorder candidates.
            let mut preseeded: Vec<_> = registry
                .charsets
                .values()
                .filter(|info| {
                    info.emacs_mule_id.is_some()
                        && registry.resolve_name(info.name) == info.name
                        && !seen.contains(&info.name)
                        && !registry.priority.contains(&info.name)
                })
                .collect();
            preseeded.sort_by_key(|info| info.id);
            for info in preseeded {
                candidates.push(MuleCandidate {
                    name: info.name,
                    id: info.emacs_mule_id.expect("filtered Mule charset"),
                    dimension: info.dimension,
                    encoder: None,
                });
            }
            candidates
        });
        Self { candidates }
    }

    /// The first encodable member of GNU's current Mule list, with its Mule id,
    /// dimension and scalar code. Unlike the legacy id-sort approximation,
    /// priority changes may select any member, including deprecated JIS-1978.
    #[inline]
    pub(crate) fn encode_char(&mut self, ch: i64) -> Option<(i64, i64, i64)> {
        self.candidates.iter_mut().find_map(|candidate| {
            let encoder = candidate
                .encoder
                .get_or_insert_with(|| CharsetEncoder::new(candidate.name));
            encoder
                .encode_char(ch)
                .map(|code| (candidate.id, candidate.dimension, code))
        })
    }
}
