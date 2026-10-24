//! Reserved fresh-ID publication. No serial intern, installed-heap lookup or
//! managed allocation runs under this guard. Existing name atoms never move.
use super::*;
use crate::tagged::gc::HeapIdentity;
use std::mem::{MaybeUninit, size_of};

/// An exclusive borrow prevents a prepared ID/reservation from becoming stale
/// before commit. It carries the original registry guard and matching WRITER.
pub(crate) struct PreparedFreshCanonical<'writer, 'authority> {
    writer: &'writer mut InitialRegistryWriter<'authority>,
    data: PreparedData,
}
static_assertions::assert_not_impl_any!(PreparedFreshCanonical<'static, 'static>: Send, Sync, Clone, Copy);

struct PreparedData {
    metadata: ExistingCanonicalSymbol,
    exact: SymbolNameValue,
    atom: Option<PreparedAtom>,
    root_bucket: FxHashMap<SymbolNameObjectId, HeapNameWord>,
    next_epoch: u64,
}

/// Existing slots derive from retained process roots; a new chunk keeps its
/// ownership until install derives the ONE process-lifetime provenance root.
/// No pointer into a movable Vec spine or a subsequently moved Box is retained.
struct PreparedAtom {
    value: LispString,
    destination: AtomDestination,
    next_len: usize,
}

enum AtomDestination {
    RetainedSlot(NonNull<LispString>),
    DetachedChunk(Box<[MaybeUninit<LispString>]>),
}

impl PreparedAtom {
    fn new(storage: &mut NameAtomStorage, name: &LispString) -> Result<Self, RegistrationError> {
        let next_len = storage
            .len
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        let index = storage.len / NAME_ATOM_CHUNK;
        let offset = storage.len % NAME_ATOM_CHUNK;
        let destination = if offset == 0 {
            if index != storage.chunks.len() {
                return Err(RegistrationError::Publication);
            }
            storage
                .chunks
                .try_reserve(1)
                .map_err(|_| RegistrationError::Capacity)?;
            let mut detached = Vec::new();
            detached
                .try_reserve_exact(NAME_ATOM_CHUNK)
                .map_err(|_| RegistrationError::Capacity)?;
            detached.resize_with(NAME_ATOM_CHUNK, MaybeUninit::<LispString>::uninit);
            AtomDestination::DetachedChunk(detached.into_boxed_slice())
        } else {
            let chunk = storage
                .chunks
                .get(index)
                .ok_or(RegistrationError::Publication)?;
            // SAFETY: the retained chunk contains NAME_ATOM_CHUNK slots and
            // offset is its next uninitialized slot, below that bound.
            AtomDestination::RetainedSlot(unsafe {
                NonNull::new_unchecked(chunk.as_ptr().add(offset))
            })
        };
        // Ordinary Rust copies may abort on allocator exhaustion, just like
        // detached World chunks. Recoverable try_reserve refusals precede any
        // logical mutation; no heap property Values enter process storage.
        let spelling = name.without_properties();
        let value = normalize_symbol_name_lisp_string(&spelling).into_owned();
        Ok(Self {
            value,
            destination,
            next_len,
        })
    }

    fn install(self, storage: &mut NameAtomStorage) -> NameAtomRef {
        let slot = match self.destination {
            AtomDestination::RetainedSlot(slot) => slot,
            AtomDestination::DetachedChunk(chunk) => {
                // SAFETY: nonempty, checked detached allocation. Derive the
                // root AFTER Box ownership ends; use this same root for its
                // first slot and every later append. Spine reserve succeeded.
                let raw = Box::into_raw(chunk).cast::<LispString>();
                let root = unsafe { NonNull::new_unchecked(raw) };
                storage.chunks.push(root);
                root
            }
        };
        // SAFETY: exclusive registry guard, a checked next slot, no previous
        // initialization, and property-free owned bytes retained forever.
        unsafe { slot.as_ptr().write(self.value) };
        storage.len = self.next_len;
        NameAtomRef(unsafe { slot.as_ref() })
    }
}

// Known retained Rust payload capacities only, excluding allocator/control
// bytes. A winning actor charges shared registry growth once, never per World
// member or losing contender. Detached preparations are not retained charges.
fn storage_bytes(registry: &SymbolRegistry) -> usize {
    fn map_bytes<K, V>(map: &FxHashMap<K, V>) -> usize {
        map.capacity().saturating_mul(size_of::<(K, V)>())
    }
    registry
        .symbols
        .capacity()
        .saturating_mul(size_of::<SymbolSlot>())
        .saturating_add(map_bytes(&registry.canonical_by_name))
        .saturating_add(map_bytes(&registry.name_values))
        .saturating_add(
            registry
                .names
                .map
                .capacity()
                .saturating_mul(size_of::<(SymbolNameKey, NameId)>()),
        )
        .saturating_add(
            registry
                .names
                .strings
                .chunks
                .capacity()
                .saturating_mul(size_of::<NonNull<LispString>>()),
        )
        .saturating_add(
            registry
                .names
                .strings
                .chunks
                .len()
                .saturating_mul(NAME_ATOM_CHUNK.saturating_mul(size_of::<LispString>())),
        )
        .saturating_add(map_bytes(&registry.name_value_roots.by_heap))
        .saturating_add(
            registry
                .name_value_roots
                .by_heap
                .values()
                .map(map_bytes)
                .fold(0usize, usize::saturating_add),
        )
}

impl PreparedData {
    fn new(
        registry: &mut SymbolRegistry,
        value: TaggedValue,
        heap: HeapIdentity,
    ) -> Result<Self, RegistrationError> {
        let name = value
            .as_lisp_string()
            .ok_or(RegistrationError::Publication)?;
        if registry.lookup_lisp_string(name).is_some() {
            return Err(RegistrationError::Publication);
        }
        let id =
            SymId(u32::try_from(registry.symbols.len()).map_err(|_| RegistrationError::Exhausted)?);
        let next_epoch = symbol_registry_epoch_value()
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        // A removed canonical mapping can leave an existing immutable atom.
        // That spelling reuses NameId, while the new canonical SymId is fresh.
        let (name_id, atom) = match registry.names.lookup_lisp_string(name) {
            Some(name_id) => (name_id, None),
            None => {
                let name_id = NameId(
                    u32::try_from(registry.names.strings.len())
                        .map_err(|_| RegistrationError::Exhausted)?,
                );
                if name_id == crate::emacs_core::symbol::SYMBOL_NAME_SENTINEL {
                    return Err(RegistrationError::Exhausted);
                }
                registry
                    .names
                    .map
                    .try_reserve(1)
                    .map_err(|_| RegistrationError::Capacity)?;
                (
                    name_id,
                    Some(PreparedAtom::new(&mut registry.names.strings, name)?),
                )
            }
        };
        registry
            .symbols
            .try_reserve(1)
            .map_err(|_| RegistrationError::Capacity)?;
        registry
            .canonical_by_name
            .try_reserve(1)
            .map_err(|_| RegistrationError::Capacity)?;
        registry
            .name_values
            .try_reserve(1)
            .map_err(|_| RegistrationError::Capacity)?;
        let exact = SymbolNameValue {
            word: HeapNameWord::of(value),
            heap_id: SymbolNameHeapId(heap.get()),
        };
        registry
            .name_value_roots
            .by_heap
            .try_reserve(1)
            .map_err(|_| RegistrationError::Capacity)?;
        let mut root_bucket = FxHashMap::default();
        if let Some(bucket) = registry.name_value_roots.by_heap.get_mut(&exact.heap_id) {
            bucket
                .try_reserve(1)
                .map_err(|_| RegistrationError::Capacity)?;
        } else {
            root_bucket
                .try_reserve(1)
                .map_err(|_| RegistrationError::Capacity)?;
        }
        Ok(Self {
            metadata: ExistingCanonicalSymbol { id, name: name_id },
            exact,
            atom,
            root_bucket,
            next_epoch,
        })
    }
}

impl<'authority> InitialRegistryWriter<'authority> {
    /// The validated input home and explicit era heap supply the exact name
    /// identity. This method changes capacities only; dropping the result
    /// cannot publish a name, ID, root, mapping or epoch.
    pub(crate) fn prepare_fresh<'writer>(
        &'writer mut self,
        value: TaggedValue,
        heap: HeapIdentity,
        charged: &mut usize,
    ) -> Result<PreparedFreshCanonical<'writer, 'authority>, RegistrationError> {
        let before = storage_bytes(&self.registry);
        let data = PreparedData::new(&mut self.registry, value, heap);
        *charged = charged.saturating_add(storage_bytes(&self.registry).saturating_sub(before));
        Ok(PreparedFreshCanonical {
            writer: self,
            data: data?,
        })
    }
}

impl PreparedFreshCanonical<'_, '_> {
    pub(crate) fn metadata(&self) -> ExistingCanonicalSymbol {
        self.data.metadata
    }

    /// Call only after the World member's arms, presence and counters are
    /// installed. All capacity checks and allocations completed in prepare.
    /// Primitive keys cannot invoke user code. Canonical publication is last
    /// among structural stores, and no fallible edge follows it. Thus an
    /// independent registry reader cannot see half a transaction, even though
    /// it does not participate in this World's authority word.
    pub(crate) fn commit(self, charged: &mut usize) {
        let Self { writer, data } = self;
        let registry = &mut *writer.registry;
        let before = storage_bytes(registry);
        let atom_bytes = data.atom.as_ref().map_or(0, |atom| atom.value.sbytes());
        if let Some(atom) = data.atom {
            let atom = atom.install(&mut registry.names.strings);
            registry
                .names
                .map
                .insert(SymbolNameKey(atom), data.metadata.name);
        }
        registry.symbols.push(SymbolSlot {
            name: data.metadata.name,
            canonical: true,
            name_origin: SymbolNameOrigin::LispObject,
        });
        registry.name_values.insert(data.metadata.id, data.exact);
        registry
            .name_value_roots
            .by_heap
            .entry(data.exact.heap_id)
            .or_insert(data.root_bucket)
            .insert(SymbolNameObjectId::of(data.exact.word), data.exact.word);
        registry
            .canonical_by_name
            .insert(data.metadata.name, data.metadata.id);
        // C5 deliberately publishes one epoch per fresh ID. Serial append's
        // narrower positive-cache invalidation policy remains unchanged.
        symbol_registry_epoch().store(data.next_epoch, Ordering::Release);
        *charged = charged
            .saturating_add(storage_bytes(registry).saturating_sub(before))
            .saturating_add(atom_bytes);
    }
}

/// Scalar-only observations of a proposed ID and exact heap/name root. No
/// registry storage or managed value escapes this assertion-only snapshot.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FreshRegistryFacts {
    pub(crate) epoch: u64,
    pub(crate) symbols: usize,
    pub(crate) atoms: usize,
    pub(crate) atom_names: usize,
    pub(crate) canonical_names: usize,
    pub(crate) name_values: usize,
    pub(crate) heap_buckets: usize,
    pub(crate) heap_names: usize,
    pub(crate) input_rooted: bool,
    pub(crate) id_present: bool,
    pub(crate) canonical_id: Option<SymId>,
    pub(crate) atom_present: bool,
    pub(crate) bytes: usize,
}

#[cfg(test)]
pub(crate) fn fresh_registry_facts_for_test(
    heap: HeapIdentity,
    id: SymId,
    input: usize,
    spelling: &str,
) -> FreshRegistryFacts {
    let registry = global_symbol_registry().read();
    let roots = registry
        .name_value_roots
        .by_heap
        .get(&SymbolNameHeapId(heap.get()));
    FreshRegistryFacts {
        epoch: symbol_registry_epoch_value(),
        symbols: registry.symbols.len(),
        atoms: registry.names.strings.len(),
        atom_names: registry.names.map.len(),
        canonical_names: registry.canonical_by_name.len(),
        name_values: registry.name_values.len(),
        heap_buckets: registry.name_value_roots.by_heap.len(),
        heap_names: roots.map_or(0, |roots| roots.len()),
        input_rooted: roots.is_some_and(|roots| roots.contains_key(&SymbolNameObjectId(input))),
        id_present: registry.slot(id).is_some(),
        canonical_id: registry.lookup(spelling),
        atom_present: registry.names.lookup(spelling).is_some(),
        bytes: storage_bytes(&registry),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p3sym_c5_name_atom_chunk_publication_keeps_one_provenance_root() {
        let mut storage = NameAtomStorage::new();
        let name = LispString::from_unibyte_slice(b"p3sym-c5-name-chunk");
        // Empty storage crosses a real chunk boundary without filling 4096
        // unrelated atoms in a provenance interpreter. The prepared owning
        // Box moves before install; the first slot derives only after leak.
        let first = PreparedAtom::new(&mut storage, &name).unwrap();
        assert_eq!((storage.len, storage.chunks.len()), (0, 0));
        let first = first.install(&mut storage);
        let address = first.0 as *const LispString;
        let second = PreparedAtom::new(&mut storage, &name).unwrap();
        let second = second.install(&mut storage);
        assert_eq!((storage.len, storage.chunks.len()), (2, 1));
        assert_eq!(address, storage.get(NameId(0)) as *const LispString);
        assert_ne!(address, second.0 as *const LispString);
        assert_eq!(first.0.as_bytes(), name.as_bytes());
        assert_eq!(second.0.as_bytes(), name.as_bytes());
        assert!(!first.0.has_intervals() && !second.0.has_intervals());
    }

    #[test]
    fn p3sym_c5_abandoned_name_atom_preparations_leave_no_publication() {
        let mut storage = NameAtomStorage::new();
        let name = LispString::from_unibyte_slice(b"p3sym-c5-abandoned-name-chunk");
        let detached = PreparedAtom::new(&mut storage, &name).unwrap();
        assert!(matches!(
            &detached.destination,
            AtomDestination::DetachedChunk(_)
        ));
        drop(detached);
        assert_eq!((storage.len, storage.chunks.len()), (0, 0));
        let first = PreparedAtom::new(&mut storage, &name)
            .unwrap()
            .install(&mut storage);
        let address = first.0 as *const LispString;
        let retained = PreparedAtom::new(&mut storage, &name).unwrap();
        assert!(matches!(
            &retained.destination,
            AtomDestination::RetainedSlot(_)
        ));
        drop(retained);
        assert_eq!((storage.len, storage.chunks.len()), (1, 1));
        assert_eq!(first.0.as_bytes(), name.as_bytes());
        let second = PreparedAtom::new(&mut storage, &name)
            .unwrap()
            .install(&mut storage);
        assert_eq!((storage.len, storage.chunks.len()), (2, 1));
        assert_eq!(address, storage.get(NameId(0)) as *const LispString);
        assert_ne!(address, second.0 as *const LispString);
        assert_eq!(second.0.as_bytes(), name.as_bytes());
        eprintln!("P3SYM_C5_ABANDONED_ATOM detached=1 retained=1 published=2");
    }
}
