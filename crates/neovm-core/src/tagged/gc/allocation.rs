//! Allocation: every `alloc_*` entry point of the GC-managed heap (conses, strings, floats, vectors, char-tables, hash tables, obarrays, lambdas, macros, records).
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

impl TaggedHeap {
    /// Allocate a cons cell. Returns a tagged Value.
    pub fn alloc_cons(&mut self, car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::ConsCells, 1);
        // Allocate-black during the deferred sweep OR a concurrent mark: a cons
        // born while a block is unswept must survive that block's reclaim, and a
        // cons born during concurrent marking must survive this cycle's sweep
        // (the GC thread won't reach it, and a black owner may point at it before
        // the next root snapshot). New conses are always live, so this is exact
        // (cleared at the next mark's begin).
        let sweeping = self.sweep_in_progress || self.concurrent_mark_running;
        if !self.cons_free_list.is_null() {
            let cell = self.cons_free_list;
            unsafe {
                self.cons_free_list = (*cell).free_next();
                (*cell).set_car(car);
                (*cell).set_cdr(cdr);
            }
            self.allocated_count += 1;
            self.cons_live_count += 1;
            self.note_allocation_bytes(size_of::<ConsCell>());
            if sweeping {
                self.mark_cons(cell);
            }
            return unsafe { TaggedValue::from_cons_ptr(cell) };
        }

        if let Some(block) = self.cons_blocks.last_mut()
            && let Some(cell) = block.alloc_bump(car, cdr)
        {
            if sweeping {
                block.mark_ptr(cell);
            }
            self.allocated_count += 1;
            self.cons_live_count += 1;
            self.note_allocation_bytes(size_of::<ConsCell>());
            return unsafe { TaggedValue::from_cons_ptr(cell) };
        }

        // All existing blocks are exhausted and there are no reclaimed cells,
        // so allocate a fresh current block and bump from it, matching GNU's
        // cons_block/cons_block_index path.
        let mut block = ConsBlock::new();
        let block_base = block.base_addr();
        let cell = block
            .alloc_bump(car, cdr)
            .expect("fresh block should have space");
        self.cons_blocks.push(block);
        let block_index = self.cons_blocks.len() - 1;
        self.cons_block_index_by_base
            .insert(block_base, block_index);
        self.allocated_count += 1;
        self.cons_live_count += 1;
        self.note_allocation_bytes(size_of::<ConsCell>());
        if sweeping {
            self.mark_cons(cell);
        }
        unsafe { TaggedValue::from_cons_ptr(cell) }
    }

    /// Allocate a string object from the STRING ARENA PAGES.
    ///
    /// Every slot allocation/reuse performs a FULL-header `ptr::write` of the
    /// whole 56-byte `StringObj` — a fresh `GcHeader` (kind=String,
    /// tenured=false, next=null) plus the moved-in `LispString`, whose
    /// `intervals` `AtomicPtr` word overwrites any STALE interval pointer
    /// left by the slot's previous occupant BEFORE the value is published
    /// (for a fresh `LispString` that word is null; a leaked stale non-null
    /// word would be taken for a live table by the GC thread's null-check
    /// and dereferenced by `mark_value`'s interval trace — a UAF). Writing
    /// the atomic word non-atomically inside `ptr::write` is sound: the slot
    /// is unreachable by any other thread until the tagged value escapes.
    /// Then the same unconditional born-at-parity store `link_object`
    /// applies.
    ///
    /// Page strings are OWNED via the page-span oracle: they NEVER touch
    /// `all_objects`, `non_cons_object_addrs`, or `link_object` — the
    /// intrusive lists sweep with `free_gc_object`/`Box::from_raw`, which
    /// would corrupt the heap on a page pointer. The page sweep is the only
    /// string reclaimer (it `drop_in_place`s dead slots, freeing the byte
    /// storage and interval table the string owns).
    pub fn alloc_string(&mut self, s: crate::heap_types::LispString) -> TaggedValue {
        let empty_kind = (s.sbytes() == 0).then(|| s.storage_kind());
        if let Some(value) = empty_kind.and_then(|kind| self.canonical_empty_strings.get(kind)) {
            return value;
        }

        self.add_memory_use_count(MemoryUseCountSlot::Strings, 1);
        self.add_memory_use_count(MemoryUseCountSlot::StringChars, s.sbytes() as u64);
        let ptr = self.string_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                StringObj {
                    header: GcHeader::new(HeapObjectKind::String),
                    data: s,
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_object`): allocate-black during a mark/sweep, pre-armed
            // white for the next `begin_collection` flip otherwise.
            (*ptr).header.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::string_object_bytes(&*ptr) });
        let value = unsafe { TaggedValue::from_string_ptr(ptr) };
        if let Some(kind) = empty_kind {
            self.canonical_empty_strings.install_owned(kind, value)
        } else {
            value
        }
    }

    /// Allocate a float object from the FLOAT ARENA PAGES.
    ///
    /// Every slot allocation/reuse performs a FULL-HEADER WRITE — a complete
    /// `FloatObj` (fresh `GcHeader`: kind=Float, tenured=false, next=null)
    /// followed by the same unconditional born-at-parity store `link_object`
    /// applies. A reused slot's stale bytes must never leak into the new
    /// object: a stale mark bit is a same-cycle-reuse UAF, a stale kind is a
    /// type-confused free, a stale tenured flag is a leak plus child-UAF
    /// (never traced, never swept).
    ///
    /// Page floats are OWNED via the page-span oracle (stage-3 fold-in: they
    /// no longer touch `non_cons_object_addrs` — `mark_value`'s
    /// owned-vs-mapped routing and `is_heap_young` answer through
    /// `float_arena.owns`) and are NOT `link_object`ed — the intrusive lists
    /// sweep with `free_gc_object`/`Box::from_raw`, which would corrupt the
    /// heap on a page pointer. The page sweep is the only float reclaimer.
    pub fn alloc_float(&mut self, value: f64) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::Floats, 1);
        let ptr = self.float_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                FloatObj {
                    header: GcHeader::new(HeapObjectKind::Float),
                    value,
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_object`): allocate-black during a mark/sweep, pre-armed
            // white for the next `begin_collection` flip otherwise.
            (*ptr).header.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<FloatObj>());
        unsafe { TaggedValue::from_float_ptr(ptr) }
    }

    /// Allocate a vector from the VECTOR ARENA PAGES.
    ///
    /// This is the single `VecLikeType::Vector` allocation chokepoint (every
    /// other veclike stays a `Box` through `link_veclike`). One FULL-header
    /// `ptr::write` of the whole 48-byte `VectorObj` (fresh `VecLikeHeader`
    /// plus the `LispValueVec` built from `items` — a reused slot's stale
    /// bytes never leak), then the unconditional born-at-parity store.
    ///
    /// INCREMENTAL VECTOR REGISTRY (Fix A) at the page chokepoint: page
    /// vectors never pass `link_veclike`, so the registry insert lives HERE
    /// and the matching remove lives in the page sweep's free hook
    /// (`sweep_arena_pages_ranges`) — the Tier-B vecsnap keeps enumerating
    /// every live vector. Page vectors never touch `all_objects` /
    /// `non_cons_object_addrs`; the page sweep is their only reclaimer (its
    /// `drop_in_place` frees the element `Vec` the vector owns).
    pub fn alloc_vector(&mut self, items: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, items.len() as u64);
        let ptr = self.vector_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                VectorObj {
                    header: VecLikeHeader::new(VecLikeType::Vector),
                    data: items.into(),
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_veclike`).
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        let registered = self.vector_object_addrs.insert(ptr as usize);
        debug_assert!(
            registered,
            "page vector allocated twice (bitmap/registry out of sync)"
        );
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(
            size_of::<VectorObj>()
                .saturating_add(Self::lisp_value_vec_storage_bytes(unsafe { &(*ptr).data })),
        );
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GNU-shaped char-table.
    pub fn alloc_char_table(
        &mut self,
        purpose: TaggedValue,
        init: TaggedValue,
        n_extras: usize,
    ) -> TaggedValue {
        let contents = [init; CHAR_TABLE_TOP_SLOTS];
        let extras = vec![init; n_extras];
        self.add_memory_use_count(
            MemoryUseCountSlot::VectorCells,
            (4 + CHAR_TABLE_TOP_SLOTS + n_extras) as u64,
        );
        let obj = Box::new(CharTableObj {
            header: VecLikeHeader::new(VecLikeType::CharTable),
            defalt: init,
            parent: TaggedValue::NIL,
            purpose,
            ascii: init,
            contents,
            extras: extras.into(),
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe {
            size_of::<CharTableObj>()
                .saturating_add(Self::lisp_value_vec_storage_bytes(&(*ptr).extras))
        });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GNU-shaped sub-char-table.
    pub fn alloc_sub_char_table(
        &mut self,
        depth: i32,
        min_char: i32,
        contents: Vec<TaggedValue>,
    ) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, contents.len() as u64);
        let obj = Box::new(SubCharTableObj {
            header: VecLikeHeader::new(VecLikeType::SubCharTable),
            depth,
            min_char,
            contents: contents.into(),
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe {
            size_of::<SubCharTableObj>()
                .saturating_add(Self::lisp_value_vec_storage_bytes(&(*ptr).contents))
        });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a hash table.
    pub fn alloc_hash_table(
        &mut self,
        table: crate::emacs_core::value::LispHashTable,
    ) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, 1);
        let obj = Box::new(HashTableObj {
            header: VecLikeHeader::new(VecLikeType::HashTable),
            table,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::hash_table_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GNU-shaped obarray object.
    pub fn alloc_obarray(&mut self, buckets: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, buckets.len() as u64);
        let obj = Box::new(ObarrayObj {
            header: VecLikeHeader::new(VecLikeType::Obarray),
            buckets: buckets.into(),
            count: 0,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::obarray_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a lambda.
    /// Allocate a lambda (interpreted closure) as a Value vector.
    /// Matches GNU Emacs's PVEC_CLOSURE: all slots are GC-traced Values.
    ///
    /// Allocated from the LAMBDA ARENA PAGES (task 03/3b): one FULL-header
    /// `ptr::write` of the whole `LambdaObj` (a reused slot's stale bytes
    /// never leak — a stale kind would type-confuse the Drop of garbage
    /// `Vec`/`OnceLock` pointers), then the unconditional born-at-parity
    /// store. Page lambdas are OWNED via the page-span oracle
    /// (`lambda_arena.owns`, routed by `owns_veclike_object`): they NEVER
    /// touch `all_objects` / `non_cons_object_addrs` / `link_veclike` — the
    /// intrusive lists sweep with `free_gc_object`/`Box::from_raw`, which
    /// would corrupt the heap on a page pointer. The page sweep is the only
    /// lambda reclaimer (its `drop_in_place` frees the closure slot `Vec` +
    /// the cached `LambdaParams`). MARKING IS UNCHANGED — the GC thread still
    /// defers every lambda to the STW termination drain (concurrent claiming
    /// is a future task); `mark_value`'s owned veclike arm traces it as before.
    pub fn alloc_lambda(&mut self, slots: Vec<TaggedValue>) -> TaggedValue {
        let ptr = self.lambda_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                LambdaObj {
                    header: VecLikeHeader::new(VecLikeType::Lambda),
                    data: slots.into(),
                    parsed_params: std::sync::OnceLock::new(),
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::lambda_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a lambda from a LambdaData (bridge for migration).
    /// Converts LambdaData fields to the Value vector layout.
    pub fn alloc_lambda_from_data(
        &mut self,
        data: crate::emacs_core::value::LambdaData,
    ) -> TaggedValue {
        let slots = data.to_closure_slots();
        self.alloc_lambda(slots)
    }

    /// Allocate a macro as a Value vector, from the MACRO ARENA PAGES (task
    /// 03/3b — same discipline as `alloc_lambda`, own arena at the shared
    /// 128B stride; `drop_in_place` frees the slot `Vec` + cached params).
    pub fn alloc_macro(&mut self, slots: Vec<TaggedValue>) -> TaggedValue {
        let ptr = self.macro_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                MacroObj {
                    header: VecLikeHeader::new(VecLikeType::Macro),
                    data: slots.into(),
                    parsed_params: std::sync::OnceLock::new(),
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::macro_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a macro from a LambdaData (bridge for migration).
    pub fn alloc_macro_from_data(
        &mut self,
        data: crate::emacs_core::value::LambdaData,
    ) -> TaggedValue {
        let slots = data.to_closure_slots();
        self.alloc_macro(slots)
    }

    /// Allocate a buffer reference.
    pub fn alloc_buffer(&mut self, id: crate::buffer::BufferId) -> TaggedValue {
        let obj = Box::new(BufferObj {
            header: VecLikeHeader::new(VecLikeType::Buffer),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<BufferObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a window reference.
    pub fn alloc_window(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(WindowObj {
            header: VecLikeHeader::new(VecLikeType::Window),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<WindowObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a frame reference.
    pub fn alloc_frame(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(FrameObj {
            header: VecLikeHeader::new(VecLikeType::Frame),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<FrameObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a timer reference.
    pub fn alloc_timer(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(TimerObj {
            header: VecLikeHeader::new(VecLikeType::Timer),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<TimerObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a process reference.
    pub fn alloc_process(&mut self, id: crate::emacs_core::process::ProcessId) -> TaggedValue {
        let obj = Box::new(ProcessObj {
            header: VecLikeHeader::new(VecLikeType::Process),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<ProcessObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a display terminal object.
    pub fn alloc_terminal(&mut self, id: u64) -> TaggedValue {
        let obj = Box::new(TerminalObj {
            header: VecLikeHeader::new(VecLikeType::Terminal),
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<TerminalObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate an xwidget model object.
    pub fn alloc_xwidget(
        &mut self,
        type_: TaggedValue,
        title: TaggedValue,
        buffer: TaggedValue,
        width: i32,
        height: i32,
        xwidget_id: u32,
        webview_id: neomacs_display_protocol::WebViewId,
    ) -> TaggedValue {
        let obj = Box::new(XwidgetObj {
            header: VecLikeHeader::new(VecLikeType::Xwidget),
            plist: TaggedValue::NIL,
            type_,
            buffer,
            title,
            script_callbacks: TaggedValue::NIL,
            height,
            width,
            xwidget_id,
            webview_id,
            kill_without_query: false,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<XwidgetObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GC-managed shader-surface handle.
    ///
    /// Deliberately NOT registry-rooted (contrast `alloc_finalizer` /
    /// xwidgets' `internal_xwidget_list`): the handle dies when Lisp drops
    /// it, and `free_gc_object` then queues `surface_id` on
    /// `pending_surface_destroys` for the evaluator's post-collection drain.
    pub fn alloc_surface_handle(&mut self, surface_id: u32) -> TaggedValue {
        let obj = Box::new(SurfaceObj {
            header: VecLikeHeader::new(VecLikeType::SurfaceHandle),
            surface_id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<SurfaceObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a GC-managed video-session handle.
    pub fn alloc_video_handle(
        &mut self,
        video_id: neomacs_display_protocol::VideoId,
    ) -> TaggedValue {
        let obj = Box::new(VideoObj {
            header: VecLikeHeader::new(VecLikeType::VideoHandle),
            video_id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<VideoObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Take the surface ids of handles the sweep reclaimed since the last
    /// drain. The evaluator's cycle-completed block queues a best-effort
    /// `DisplayHost::destroy_shader_surface` for each.
    pub fn take_pending_surface_destroys(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_surface_destroys)
    }

    /// Take video ids whose last Lisp handle was reclaimed by the sweep.
    pub fn take_pending_video_destroys(&mut self) -> Vec<neomacs_display_protocol::VideoId> {
        std::mem::take(&mut self.pending_video_destroys)
    }

    /// Allocate an xwidget view object.
    pub fn alloc_xwidget_view(&mut self, model: TaggedValue, window: TaggedValue) -> TaggedValue {
        let obj = Box::new(XwidgetViewObj {
            header: VecLikeHeader::new(VecLikeType::XwidgetView),
            model,
            window,
            x: 0,
            y: 0,
            clip_right: 0,
            clip_bottom: 0,
            clip_top: 0,
            clip_left: 0,
            redisplayed: false,
            hidden: false,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<XwidgetViewObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a bytecode function from the BYTECODE ARENA PAGES.
    ///
    /// This is the single `VecLikeType::ByteCode` allocation chokepoint —
    /// every producer (`Value::make_bytecode`, the pdump restore placeholder
    /// at `DumpHeapObject::ByteCode`) funnels here. One FULL-header
    /// `ptr::write` of the whole `ByteCodeObj` (fresh `VecLikeHeader` plus
    /// the moved-in `ByteCodeFunction` — a reused slot's stale bytes never
    /// leak: a stale kind is a type-confused Drop of garbage `Vec` pointers),
    /// then the unconditional born-at-parity store (the `link_veclike` seam's
    /// store).
    ///
    /// Page bytecode is OWNED via the page-span oracle
    /// (`bytecode_arena.owns`, routed by `owns_veclike_object`): it NEVER
    /// touches `all_objects` / `non_cons_object_addrs` / `link_veclike` — the
    /// intrusive lists sweep with `free_gc_object`/`Box::from_raw`, which
    /// would corrupt the heap on a page pointer. The page sweep is the only
    /// bytecode reclaimer (its `drop_in_place` frees the ops/constants
    /// vectors, params, GNU byte maps, and docstring the function owns).
    ///
    /// MARKING (task 01 bytecode arm): the GC thread CLAIMS page bytecode
    /// discovered during a concurrent mark (page-base snapshot hit +
    /// `mark_claim_at`) and gray-pushes its children right there — sound
    /// because published bytecode is immutable (compile-time enforced; see
    /// the claim arm in `concurrent_try_mark_owned`). Snapshot misses
    /// (mid-cycle pages, mapped/dump residue) still defer to the STW
    /// termination drain, where `mark_value`'s owned veclike arm traces
    /// them exactly as before.
    pub fn alloc_bytecode(
        &mut self,
        data: crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> TaggedValue {
        let ptr = self.bytecode_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                ByteCodeObj {
                    header: VecLikeHeader::new(VecLikeType::ByteCode),
                    data,
                },
            );
            // BORN-AT-PARITY, unconditionally — the link seam's store (see
            // `link_veclike`).
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::bytecode_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a record.
    ///
    /// Allocated from the RECORD ARENA PAGES (task 03/3b): the single
    /// `RecordObj` allocation chokepoint alongside `alloc_window_configuration`
    /// (same Rust type, distinct tag — both funnel to `record_arena`). One
    /// FULL-header `ptr::write` (a stale kind would type-confuse the Drop of
    /// the garbage slot `Vec`), unconditional born-at-parity store; NO
    /// intrusive-list / addr-set entry (owned via the page-span oracle,
    /// routed by `owns_veclike_object`). The page sweep's `drop_in_place`
    /// frees the record's slot `Vec`. Marking is unchanged (deferred).
    pub fn alloc_record(&mut self, items: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, items.len() as u64);
        self.alloc_record_like(VecLikeType::Record, items)
    }

    /// Allocate a native opened-font pseudovector (`PVEC_FONT`).  Fonts retain
    /// typed metrics and an exact backend identity, so they are residual
    /// boxed objects rather than pretending to be record slots.
    pub fn alloc_font(&mut self, data: FontObjectData) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, data.fields.len() as u64);
        let obj = Box::new(FontObj {
            header: VecLikeHeader::new(VecLikeType::Font),
            data,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::font_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a window configuration. Structurally a record (`{header, data}`)
    /// but tagged `WindowConfiguration` so it is a distinct pseudovector type.
    /// Shares the record arena (same `RecordObj`).
    pub fn alloc_window_configuration(&mut self, items: Vec<TaggedValue>) -> TaggedValue {
        self.add_memory_use_count(MemoryUseCountSlot::VectorCells, items.len() as u64);
        self.alloc_record_like(VecLikeType::WindowConfiguration, items)
    }

    /// Shared `RecordObj` page allocator for the `Record` and
    /// `WindowConfiguration` tags. `add_memory_use_count` is the caller's job
    /// (both currently count `VectorCells`).
    pub(super) fn alloc_record_like(
        &mut self,
        tag: VecLikeType,
        items: Vec<TaggedValue>,
    ) -> TaggedValue {
        debug_assert!(matches!(
            tag,
            VecLikeType::Record | VecLikeType::WindowConfiguration
        ));
        let ptr = self.record_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                RecordObj {
                    header: VecLikeHeader::new(tag),
                    data: items.into(),
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(unsafe { Self::record_object_bytes(&*ptr) });
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate an overlay.
    pub fn alloc_overlay(&mut self, data: crate::heap_types::OverlayData) -> TaggedValue {
        let obj = Box::new(OverlayObj {
            header: VecLikeHeader::new(VecLikeType::Overlay),
            data,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<OverlayObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a marker.
    pub fn alloc_marker(&mut self, data: crate::heap_types::LispMarker) -> TaggedValue {
        // Page-only (GNU `marker_block`): a freed slot is reused from the
        // class free list, so `save-excursion`'s make/free churn cycles
        // through a few cache-warm slots instead of scattering `Box`es
        // across the general heap.
        let ptr = self.marker_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                MarkerObj {
                    header: VecLikeHeader::new(VecLikeType::Marker),
                    data,
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<MarkerObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a bignum (arbitrary-precision integer).
    ///
    /// Mirrors GNU `make_bignum` (`src/bignum.c:113`): the caller is
    /// responsible for ensuring the value is outside fixnum range.
    /// Use `Value::make_integer` for the canonical "fixnum-or-bignum"
    /// constructor that delegates here only when promotion is needed.
    pub fn alloc_bignum(&mut self, value: Integer) -> TaggedValue {
        let obj = Box::new(BignumObj {
            header: VecLikeHeader::new(VecLikeType::Bignum),
            value,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<BignumObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a symbol-with-pos object from the SYMBOL-WITH-POS ARENA PAGES
    /// (task 03/3b). `sym` must be a bare symbol, `pos` must be a fixnum.
    ///
    /// POD-like: `SymbolWithPosObj` is two `Copy` Values (no payload, no
    /// Drop), so the class behaves like FloatObj — the sweep/teardown
    /// `drop_in_place` walk compiles out. Still one FULL-header `ptr::write`
    /// (the ownership oracle and every header read demand fully-initialized
    /// slot bytes) + the born-at-parity store; NO intrusive-list / addr-set
    /// entry (owned via the page-span oracle, routed by owns_veclike_object;
    /// free_gc_object's SymbolWithPos arm stays the residual-Box seam).
    pub fn alloc_symbol_with_pos(&mut self, sym: TaggedValue, pos: TaggedValue) -> TaggedValue {
        let ptr = self.symbol_with_pos_arena.alloc_slot();
        unsafe {
            // FULL-HEADER WRITE: never partially reuse prior slot bytes.
            std::ptr::write(
                ptr,
                SymbolWithPosObj {
                    header: VecLikeHeader::new(VecLikeType::SymbolWithPos),
                    sym,
                    pos,
                },
            );
            // BORN-AT-PARITY, unconditionally.
            (*ptr).header.gc.set_marked(self.mark_parity);
        }
        #[cfg(test)]
        alloc_probe::record(ptr as *const GcHeader, self.non_cons_object_addrs.len());
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<SymbolWithPosObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a finalizer object (GNU `Fmake_finalizer`). Registered in
    /// `finalizer_registry` so mark termination can detect when the object
    /// becomes unreachable and queue `function` to run after that cycle.
    /// GNU accepts any object as the function; callers do not validate it.
    pub fn alloc_finalizer(&mut self, function: TaggedValue) -> TaggedValue {
        let obj = Box::new(FinalizerObj {
            header: VecLikeHeader::new(VecLikeType::Finalizer),
            function,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.finalizer_registry.push(ptr);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<FinalizerObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate an SQLite database or statement object.
    pub fn alloc_sqlite(&mut self, is_statement: bool, id: i64) -> TaggedValue {
        let obj = Box::new(SqliteObj {
            header: VecLikeHeader::new(VecLikeType::Sqlite),
            is_statement,
            id,
        });
        let ptr = Box::into_raw(obj);
        self.link_veclike(ptr as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<SqliteObj>());
        unsafe { TaggedValue::from_veclike_ptr(ptr as *const VecLikeHeader) }
    }

    /// Allocate a user-pointer object for dynamic module API.
    pub fn alloc_user_ptr(
        &mut self,
        ptr: *mut std::ffi::c_void,
        finalizer: EmacsFinalizer,
    ) -> TaggedValue {
        let obj = Box::new(UserPtrObj {
            header: VecLikeHeader::new(VecLikeType::UserPtr),
            ptr,
            finalizer,
        });
        let raw = Box::into_raw(obj);
        self.link_veclike(raw as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<UserPtrObj>());
        unsafe { TaggedValue::from_veclike_ptr(raw as *const VecLikeHeader) }
    }

    /// Allocate a module-function object for dynamic module API.
    pub fn alloc_module_function(
        &mut self,
        min_arity: isize,
        max_arity: isize,
        subr: *const std::ffi::c_void,
        data: *mut std::ffi::c_void,
        documentation: TaggedValue,
        interactive_form: TaggedValue,
    ) -> TaggedValue {
        let obj = Box::new(ModuleFunctionObj {
            header: VecLikeHeader::new(VecLikeType::ModuleFunction),
            min_arity,
            max_arity,
            subr,
            data,
            finalizer: None,
            documentation,
            interactive_form,
        });
        let raw = Box::into_raw(obj);
        self.link_veclike(raw as *mut VecLikeHeader);
        self.allocated_count += 1;
        self.note_allocation_bytes(size_of::<ModuleFunctionObj>());
        unsafe { TaggedValue::from_veclike_ptr(raw as *const VecLikeHeader) }
    }
}
