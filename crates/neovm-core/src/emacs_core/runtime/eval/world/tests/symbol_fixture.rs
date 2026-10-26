//! A minimal retained World without Context/bootstrap or setup collection.
use super::*;

impl Context {
    pub(crate) fn with_test_symbol_era<R>(
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, SymId) -> R,
    ) -> R {
        Self::with_test_symbol_backing_era(crate::tagged::gc::PlainSymbolBacking::Unprojected, body)
    }

    /// C3 setup only: the same small World, optionally projected before the
    /// era exists. No production cell accessor changes and no setup collection.
    pub(crate) fn with_test_symbol_backing_era<R>(
        kind: crate::tagged::gc::PlainSymbolBacking,
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, SymId) -> R,
    ) -> R {
        Self::with_test_symbol_backings_era(&[kind], |era, ids| body(era, ids[0]))
    }

    /// All names, spine slots and projection plans are created serially before
    /// the era exists. Running tests can only access these existing symbols.
    pub(crate) fn with_test_symbol_backings_era<R>(
        kinds: &[crate::tagged::gc::PlainSymbolBacking],
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Self::with_test_symbol_backings_init_era(kinds, |_, _| {}, body)
    }

    /// C4's real immutable `(lambda () N)` definitions are created and stored
    /// by the sole serial setup owner. The matching value homes provide
    /// rooted input operands through the already-qualified C3 read API.
    pub(crate) fn with_test_symbol_functions_era<R>(
        kind: crate::tagged::gc::PlainSymbolBacking,
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Self::with_test_symbol_functions_init_era(kind, |_, _| {}, body)
    }

    /// A fixture may change initial binding states while this setup still
    /// exclusively owns the actual Obarray. The callback ends before the era.
    pub(crate) fn with_test_symbol_functions_init_era<R>(
        kind: crate::tagged::gc::PlainSymbolBacking,
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Self::with_test_symbol_backings_init_era(
            &[kind, kind],
            |obarray, ids| {
                let lambda = TaggedValue::from_sym_id(crate::emacs_core::intern::intern("lambda"));
                for (&id, answer) in ids.iter().zip([73, 97]) {
                    let body = TaggedValue::cons(TaggedValue::fixnum(answer), TaggedValue::NIL);
                    let args = TaggedValue::cons(TaggedValue::NIL, body);
                    let definition = TaggedValue::cons(lambda, args);
                    obarray.seed_value_id(id, definition);
                    obarray.set_symbol_function_id(id, definition);
                }
                init(obarray, ids);
            },
            body,
        )
    }

    /// C5 fixture operands are created by the sole serial setup owner, then
    /// copied from existing plain cells through the qualified rooted value API.
    /// No string allocation, raw name word or mutable heap view enters the era.
    pub(crate) fn with_test_symbol_name_inputs_era<R>(
        kind: crate::tagged::gc::PlainSymbolBacking,
        spelling: &str,
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Self::with_test_symbol_name_inputs_init_era(kind, spelling, |_, _| {}, body)
    }

    pub(crate) fn with_test_symbol_name_inputs_init_era<R>(
        kind: crate::tagged::gc::PlainSymbolBacking,
        spelling: &str,
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        use crate::tagged::gc::PlainSymbolBacking;
        Self::with_test_symbol_backings_init_era(
            &[
                kind,
                PlainSymbolBacking::Unprojected,
                PlainSymbolBacking::Unprojected,
            ],
            |obarray, ids| {
                obarray.seed_value_id(ids[1], TaggedValue::string(spelling));
                obarray.seed_value_id(ids[2], TaggedValue::string(spelling));
                init(obarray, ids);
            },
            body,
        )
    }

    /// Native C5 histories: real, independently allocated name operands in
    /// existing plain holders. Registry gaps are issued only AFTER those
    /// World slots exist, so intern itself must grow the retained spine/sides.
    pub(crate) fn with_test_symbol_intern_race_era<R>(
        kind: crate::tagged::gc::PlainSymbolBacking,
        spellings: &[String],
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        let mut kinds =
            vec![crate::tagged::gc::PlainSymbolBacking::Unprojected; spellings.len() + 1];
        kinds[0] = kind;
        Self::with_test_symbol_backings_init_era(
            &kinds,
            |obarray, ids| {
                for (&id, spelling) in ids[1..].iter().zip(spellings) {
                    obarray.seed_value_id(id, TaggedValue::string(spelling));
                }
                let mut id = crate::emacs_core::intern::intern_uninterned("p3sym-c5-native-gap");
                let target = (u64::from(id.0) / 4096 + 2) * 4096 + 17;
                while u64::from(id.0) + 1 < target {
                    id = crate::emacs_core::intern::intern_uninterned("p3sym-c5-native-gap");
                }
            },
            body,
        )
    }

    fn with_test_symbol_backings_init_era<R>(
        kinds: &[crate::tagged::gc::PlainSymbolBacking],
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        use crate::tagged::gc::{AliasableBox, ParallelHeapEra, TaggedHeap};

        let heap = AliasableBox::new(TaggedHeap::new());
        // Manager construction can allocate; install only the owning root.
        crate::tagged::gc::set_tagged_heap_root(heap.root());
        let mut obarray = Obarray::new();
        let ids: Vec<_> = (0..kinds.len())
            .map(|index| {
                let id = if index == 0 {
                    crate::emacs_core::intern::intern("p3sym-c2-global")
                } else {
                    crate::emacs_core::intern::intern(&format!("p3sym-c3-global-{index}"))
                };
                obarray.seed_value_id(id, TaggedValue::fixnum(73));
                id
            })
            .collect();
        init(&mut obarray, &ids);
        let mut ready = WorldStorageBuilder {
            owned_roots: Default::default(),
            tagged_heap: heap,
            obarray,
            features: Vec::new(),
            watchers: VariableWatcherList::new(),
            active_variable_watchers: HashSet::new(),
            autoloads: AutoloadManager::new(),
            treesit: crate::emacs_core::treesit::TreeSitterManager::new(),
            frames: FrameManager::default(),
            coding_systems: CodingSystemManager::new(),
            macro_perf_stats: MacroPerfStats::default(),
            buffers: BufferManager::new_world_parts(),
            global: crate::emacs_core::eval::global_state::GlobalState::fresh(
                crate::keyboard::CommandLoop::new(),
            ),
        }
        .build();
        let world = Arc::clone(&ready.obarray.lease.0);
        for (&id, &kind) in ids.iter().zip(kinds) {
            if kind == crate::tagged::gc::PlainSymbolBacking::Unprojected {
                continue;
            }
            let mut table = world.thread_projections.borrow_mut();
            ready.obarray.with_dispatch_write(|obarray| {
                if kind == crate::tagged::gc::PlainSymbolBacking::VoidProjection {
                    obarray.seed_value_id(id, TaggedValue::UNBOUND);
                }
                // Use admission's actual plan: projecting an owned UNBOUND
                // word would issue a private descriptor instead of shared VOID.
                let plan = obarray.admission_plan(id).unwrap();
                obarray.project_admitted_cell(id, plan, &mut table);
            });
        }
        let heap = world.tagged_heap.root().as_ptr();
        // SAFETY: freshly constructed, sole serial owner, no marker or GC was
        // started; no Lisp/Context root source exists and no collection runs.
        let collector = unsafe { (*heap).drain_to_quiescent(|_| {}) }.unwrap();
        let exclusion = collector.exclude_concurrent_mark().unwrap();
        let retention = exclusion.retain().unwrap();
        drop(exclusion);
        world.baton_holder.set(Some(std::thread::current().id()));
        let keeper = Baton {
            _retention: retention,
            world,
        };
        // SAFETY: same original heap root, no collector/reference remains
        // from the preceding short exclusion extent.
        let collector = unsafe { (*heap).drain_to_quiescent(|_| {}) }.unwrap();
        // No Context exists in this fixture, hence no Context-private roots.
        // World storage is retained by the actual Baton, exactly as in C2.
        let frozen = FrozenLegacyRoots {
            heap: collector.heap_identity(),
            words: Vec::new(),
            // No Context or legacy activation exists in this minimal World.
            functions_quiescent: true,
        };
        let era = ParallelHeapEra::admit_rooted(collector, keeper, frozen)
            .unwrap_or_else(|e| panic!("minimal symbol fixture admission: {:?}", e.reason));
        body(era, ids)
        // ready owns the same World until after the body and keeper teardown.
    }
}

impl Baton {
    /// Assertion-only root ledger. Requires the real stop-all authority and
    /// returns opaque words, never cells or descriptors. The normal collector
    /// still enumerates the complete World through its production root path.
    pub(crate) fn symbol_root_ledger_for_test(
        &self,
        stopped: &mut crate::tagged::gc::AllStopped<'_>,
    ) -> Result<(Vec<usize>, Vec<usize>), crate::tagged::gc::RendezvousError> {
        use crate::tagged::gc::{RegistrationError, RendezvousError};

        if !self.held_here() {
            return Err(RegistrationError::WrongThread.into());
        }
        stopped.check_world_roots(self.parallel_heap_identity())?;
        let authority = stopped.symbols()?;
        let mut projections = Vec::new();
        authority.append_current_projection_roots(self, &mut projections)?;
        let mut symbols = Vec::new();
        authority.append_current_symbol_roots(self, &mut symbols)?;
        let projections = projections.into_iter().map(TaggedValue::bits).collect();
        let symbols = symbols.into_iter().map(TaggedValue::bits).collect();
        Ok((symbols, projections))
    }
}
