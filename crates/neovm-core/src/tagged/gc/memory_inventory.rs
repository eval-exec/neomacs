//! Stopped-world, opt-in memory inventory. None of these walks run in an
//! allocation or barrier path. Retained inventory includes not-yet-collected
//! garbage; only a final mark can establish reachability for its traced ages.

use super::*;
use serde::Serialize;
use std::mem::size_of_val;

#[derive(Debug, Default, Serialize)]
pub(super) struct AgeInventory {
    pub(super) objects: usize,
    pub(super) object_struct_bytes: usize,
    pub(super) pacing_bytes: usize,
    /// Directly-owned capacities excluding Arc-shared GNU bytecode bytes.
    /// Bignum limbs are a lower bound; nested hash keys and shared names omitted.
    pub(super) known_owned_payload_capacity_bytes: usize,
    /// Breakdown already included in known owned payload capacity.
    pub(super) known_text_property_capacity_bytes: usize,
    pub(super) known_payload_logical_bytes: usize,
    /// Shared byte buffers referenced by owners, deliberately not included in
    /// the physical accounting sum because several holders may name one Arc.
    pub(super) bytecode_shared_reference_bytes: usize,
    pub(super) marked_live_pacing_bytes: Option<usize>,
    pub(super) text_properties: TextPropertyInventory,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct Ages {
    pub(super) young: AgeInventory,
    pub(super) old: AgeInventory,
    pub(super) permanent: AgeInventory,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct ConsInventory {
    pub(super) blocks: usize,
    pub(super) backing_bytes: usize,
    pub(super) trailer_and_tail_bytes: usize,
    pub(super) occupied_cells: usize,
    pub(super) occupied_bytes: usize,
    pub(super) young_cells: usize,
    pub(super) old_cells: usize,
    pub(super) free_cells: usize,
    pub(super) free_cell_bytes: usize,
    pub(super) never_bumped_cells: usize,
    pub(super) never_bumped_bytes: usize,
    pub(super) empty_blocks: usize,
    pub(super) empty_backing_bytes: usize,
    pub(super) partial_blocks: usize,
    pub(super) full_blocks: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct ArenaInventory {
    pub(super) class: &'static str,
    pub(super) active_pages: usize,
    pub(super) active_backing_bytes: usize,
    pub(super) spare_pages: usize,
    pub(super) spare_backing_bytes: usize,
    pub(super) empty_pages: usize,
    pub(super) empty_backing_bytes: usize,
    pub(super) partial_pages: usize,
    pub(super) full_pages: usize,
    pub(super) retired_pages: usize,
    pub(super) occupied_slots: usize,
    pub(super) occupied_slot_bytes: usize,
    pub(super) reclaimed_slot_bytes: usize,
    pub(super) never_bumped_slot_bytes: usize,
    pub(super) page_tail_bytes: usize,
    pub(super) known_owned_payload_capacity_bytes: usize,
    /// Breakdown already included in known owned payload capacity.
    pub(super) known_text_property_capacity_bytes: usize,
    pub(super) known_payload_logical_bytes: usize,
    pub(super) bytecode_shared_reference_bytes: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct BoxInventory {
    pub(super) class: &'static str,
    pub(super) objects: usize,
    pub(super) object_struct_bytes: usize,
    pub(super) pacing_bytes: usize,
    pub(super) known_owned_payload_capacity_bytes: usize,
    /// Breakdown already included in known owned payload capacity.
    pub(super) known_text_property_capacity_bytes: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct BufferInventory {
    pub(super) name: &'static str,
    pub(super) elements: usize,
    pub(super) element_bytes: usize,
    pub(super) logical_bytes: usize,
    pub(super) capacity_elements: usize,
    pub(super) capacity_bytes: usize,
    /// Hash-table capacity is an entry bound, not its exact bucket allocation.
    /// None prevents entry-size estimates being presented as actual bytes.
    pub(super) hash_capacity_entries: Option<usize>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct MappedInventory {
    pub(super) conses: usize,
    pub(super) floats: usize,
    pub(super) strings: usize,
    pub(super) veclikes: usize,
    pub(super) image_object_bytes: usize,
    pub(super) copied_owned_payload_capacity_bytes: usize,
    pub(super) bytecode_shared_reference_bytes: usize,
    pub(super) marked_live_image_object_bytes: Option<usize>,
    pub(super) known_text_property_capacity_bytes: usize,
    pub(super) text_properties: TextPropertyInventory,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct MemoryInventory {
    pub(super) ages: Ages,
    pub(super) cons: ConsInventory,
    pub(super) arenas: Vec<ArenaInventory>,
    pub(super) boxed: Vec<BoxInventory>,
    pub(super) mapped: MappedInventory,
    pub(super) buffers: Vec<BufferInventory>,
    /// Actual GC-owned page/block backing, owned Box structs and known
    /// payload capacity. Excludes mapped image bytes and shared Arc buffers.
    /// This is an ownership lower bound, NOT allocator resident or OS RSS.
    pub(super) known_allocator_owned_bytes: usize,
    /// Valid only after a full final mark, before P-all changes cons old bits.
    pub(super) old_dead_pacing_bytes: Option<usize>,
    pub(super) old_dead_object_and_payload_bytes: Option<usize>,
    /// Breakdown of payload bytes, never add to known_allocator_owned_bytes.
    pub(super) text_properties: TextPropertyInventory,
}

#[derive(Clone, Copy)]
enum Age {
    Young,
    Old,
    Permanent,
}

impl Ages {
    fn get_mut(&mut self, age: Age) -> &mut AgeInventory {
        match age {
            Age::Young => &mut self.young,
            Age::Old => &mut self.old,
            Age::Permanent => &mut self.permanent,
        }
    }
}

fn add(dst: &mut usize, bytes: usize) {
    *dst = dst.saturating_add(bytes);
}

/// Caller must stop the mutator, join the marker and close all allocation
/// regions. `marks_final` additionally certifies that finalizer/weak fixpoints
/// finished, and MUST be false after promotion or sweep reset final marks.
pub(super) fn snapshot(heap: &TaggedHeap, marks_final: bool) -> MemoryInventory {
    debug_assert!(!heap.alloc_regions_open());
    debug_assert!(!heap.concurrent_mark_running);
    let full_marks = marks_final && !heap.is_minor_collection();
    let mut out = MemoryInventory::default();
    if marks_final {
        out.ages.young.marked_live_pacing_bytes = Some(0);
        out.ages.permanent.marked_live_pacing_bytes = Some(0);
    }
    if full_marks {
        out.ages.old.marked_live_pacing_bytes = Some(0);
        out.old_dead_pacing_bytes = Some(0);
        out.old_dead_object_and_payload_bytes = Some(0);
    }
    cons_inventory(heap, marks_final, full_marks, &mut out);
    macro_rules! arena {
        ($field:ident, $payload:expr) => {
            out.arenas.push(arena_inventory(
                heap,
                &heap.$field,
                marks_final,
                full_marks,
                $payload,
                &mut out.ages,
                &mut out.old_dead_pacing_bytes,
                &mut out.old_dead_object_and_payload_bytes,
            ));
        };
    }
    arena!(float_arena, |_| PayloadLayout::default());
    arena!(string_arena, |object: &StringObj| {
        string_payload_with_text_properties(&object.data)
    });
    arena!(vector_arena, |object: &VectorObj| {
        TaggedHeap::value_vec_payload_layout(&object.data)
    });
    arena!(bytecode_arena, bytecode_payload_with_text_properties);
    arena!(lambda_arena, |object: &LambdaObj| {
        TaggedHeap::closure_payload_layout(&object.data, object.parsed_params.get())
    });
    arena!(macro_arena, |object: &MacroObj| {
        TaggedHeap::closure_payload_layout(&object.data, object.parsed_params.get())
    });
    arena!(record_arena, |object: &RecordObj| {
        TaggedHeap::value_vec_payload_layout(&object.data)
    });
    arena!(symbol_with_pos_arena, |_| PayloadLayout::default());
    arena!(marker_arena, |_| PayloadLayout::default());
    arena!(bignum_arena, |object: &BignumObj| {
        TaggedHeap::bignum_payload_layout(&object.value)
    });

    // List ownership is disjoint, including each detached sweep cursor.
    for head in [
        heap.all_objects,
        heap.tenured_objects,
        heap.generational.old_objects,
        heap.generational.old_sweep_pending,
        heap.sweep_noncons_pending,
    ] {
        boxed_inventory(heap, head, marks_final, full_marks, &mut out);
    }
    out.mapped = mapped_inventory(heap, marks_final);
    out.text_properties.merge(&out.ages.young.text_properties);
    out.text_properties.merge(&out.ages.old.text_properties);
    out.text_properties
        .merge(&out.ages.permanent.text_properties);
    out.text_properties.merge(&out.mapped.text_properties);
    out.buffers = buffer_inventory(heap);
    out.known_allocator_owned_bytes = out.cons.backing_bytes;
    for class in &out.arenas {
        add(
            &mut out.known_allocator_owned_bytes,
            class.active_backing_bytes,
        );
        add(
            &mut out.known_allocator_owned_bytes,
            class.spare_backing_bytes,
        );
        add(
            &mut out.known_allocator_owned_bytes,
            class.known_owned_payload_capacity_bytes,
        );
    }
    for class in &out.boxed {
        add(
            &mut out.known_allocator_owned_bytes,
            class.object_struct_bytes,
        );
        add(
            &mut out.known_allocator_owned_bytes,
            class.known_owned_payload_capacity_bytes,
        );
    }
    add(
        &mut out.known_allocator_owned_bytes,
        out.mapped.copied_owned_payload_capacity_bytes,
    );
    // Exact vector-buffer capacities are additional, disjoint allocations;
    // hash entry bounds cannot establish their allocator bucket bytes.
    out
}

fn cons_inventory(
    heap: &TaggedHeap,
    marks_final: bool,
    full_marks: bool,
    out: &mut MemoryInventory,
) {
    let cons = &mut out.cons;
    cons.blocks = heap.cons_blocks.len();
    cons.backing_bytes = cons.blocks.saturating_mul(CONS_BLOCK_BYTES);
    cons.trailer_and_tail_bytes = cons
        .blocks
        .saturating_mul(CONS_BLOCK_BYTES - CONS_CELLS_BYTES);
    for block in &heap.cons_blocks {
        let bumped = block.next_index as usize;
        let mut occupied = 0usize;
        // The free-list poison is the allocated/free discriminator here;
        // mark bits cannot distinguish newly allocated white cells from free
        // cells, and walking car never interprets the free cdr union as Value.
        for index in 0..bumped {
            let cell = unsafe { &*block.cells_ptr().add(index) };
            if unsafe { cell.load_car() }.is_dead() {
                cons.free_cells += 1;
                continue;
            }
            occupied += 1;
            let old = block.trailer().is_old(index);
            let marked = block.trailer().is_marked(index);
            let age = if old {
                cons.old_cells += 1;
                Age::Old
            } else {
                cons.young_cells += 1;
                Age::Young
            };
            let ages = out.ages.get_mut(age);
            ages.objects += 1;
            add(&mut ages.object_struct_bytes, size_of::<ConsCell>());
            add(&mut ages.pacing_bytes, size_of::<ConsCell>());
            if marks_final
                && marked
                && let Some(live) = ages.marked_live_pacing_bytes.as_mut()
            {
                add(live, size_of::<ConsCell>());
            }
            if full_marks && old && !marked {
                add(
                    out.old_dead_pacing_bytes.as_mut().unwrap(),
                    size_of::<ConsCell>(),
                );
                add(
                    out.old_dead_object_and_payload_bytes.as_mut().unwrap(),
                    size_of::<ConsCell>(),
                );
            }
        }
        cons.occupied_cells += occupied;
        cons.never_bumped_cells += CONS_BLOCK_SIZE - bumped;
        if occupied == 0 {
            cons.empty_blocks += 1;
        } else if occupied == CONS_BLOCK_SIZE {
            cons.full_blocks += 1;
        } else {
            cons.partial_blocks += 1;
        }
    }
    cons.occupied_bytes = cons.occupied_cells.saturating_mul(size_of::<ConsCell>());
    cons.free_cell_bytes = cons.free_cells.saturating_mul(size_of::<ConsCell>());
    cons.never_bumped_bytes = cons
        .never_bumped_cells
        .saturating_mul(size_of::<ConsCell>());
    cons.empty_backing_bytes = cons.empty_blocks.saturating_mul(CONS_BLOCK_BYTES);
    debug_assert_eq!(cons.occupied_cells, heap.cons_live_count);
}

fn header_age(header: &GcHeader) -> Age {
    if header.generation.permanent() {
        Age::Permanent
    } else if header.tenured {
        Age::Old
    } else {
        Age::Young
    }
}

fn shared_bytecode_bytes(header: *const GcHeader) -> usize {
    unsafe {
        if (*header).kind == HeapObjectKind::VecLike
            && (*(header as *const VecLikeHeader)).type_tag == VecLikeType::ByteCode
        {
            (*(header as *const ByteCodeObj))
                .data
                .gnu_bytecode_bytes
                .as_ref()
                .map_or(0, LispByteVec::owned_bytes)
        } else {
            0
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn note_header(
    heap: &TaggedHeap,
    header: *const GcHeader,
    struct_bytes: usize,
    payload: PayloadLayout,
    marks_final: bool,
    full_marks: bool,
    ages: &mut Ages,
    old_dead_pacing: &mut Option<usize>,
    old_dead_storage: &mut Option<usize>,
) {
    let h = unsafe { &*header };
    let age = header_age(h);
    let pacing = TaggedHeap::object_bytes_from_header(header);
    let shared = shared_bytecode_bytes(header);
    let owned_payload = payload.capacity_bytes.saturating_sub(shared);
    let text_properties = text_property_storage(header);
    let item = ages.get_mut(age);
    add(
        &mut item.known_text_property_capacity_bytes,
        text_properties.known_owned_capacity_bytes,
    );
    item.text_properties.merge(&text_properties);
    item.objects += 1;
    add(&mut item.object_struct_bytes, struct_bytes);
    add(&mut item.pacing_bytes, pacing);
    add(&mut item.known_owned_payload_capacity_bytes, owned_payload);
    add(&mut item.known_payload_logical_bytes, payload.logical_bytes);
    add(&mut item.bytecode_shared_reference_bytes, shared);
    let live = matches!(age, Age::Permanent) || h.is_marked_at(heap.mark_parity);
    if marks_final
        && live
        && let Some(bytes) = item.marked_live_pacing_bytes.as_mut()
    {
        add(bytes, pacing);
    }
    if full_marks && matches!(age, Age::Old) && !live {
        add(old_dead_pacing.as_mut().unwrap(), pacing);
        add(
            old_dead_storage.as_mut().unwrap(),
            struct_bytes.saturating_add(owned_payload),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn arena_inventory<T: PagedObject>(
    heap: &TaggedHeap,
    arena: &ObjectArena<T>,
    marks_final: bool,
    full_marks: bool,
    payload_layout: impl Fn(&T) -> PayloadLayout,
    ages: &mut Ages,
    old_dead_pacing: &mut Option<usize>,
    old_dead_storage: &mut Option<usize>,
) -> ArenaInventory {
    let mut out = ArenaInventory {
        class: T::CLASS,
        active_pages: arena.pages.len(),
        active_backing_bytes: arena.pages.len().saturating_mul(OBJECT_PAGE_BYTES),
        spare_pages: arena.spare_storage.len(),
        spare_backing_bytes: arena.spare_storage.len().saturating_mul(OBJECT_PAGE_BYTES),
        ..ArenaInventory::default()
    };
    for page in &arena.pages {
        out.occupied_slots += page.allocated;
        out.reclaimed_slot_bytes += page.next_index.saturating_sub(page.allocated) * T::SLOT_BYTES;
        out.never_bumped_slot_bytes += (ObjectPage::<T>::SLOTS - page.next_index) * T::SLOT_BYTES;
        out.retired_pages += usize::from(page.retired);
        if page.allocated == 0 {
            out.empty_pages += 1;
        } else if page.allocated == ObjectPage::<T>::SLOTS {
            out.full_pages += 1;
        } else {
            out.partial_pages += 1;
        }
        for word in 0..ObjectPage::<T>::ALLOC_WORDS {
            let mut bits = page.alloc_bits[word];
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let object = unsafe { &*page.slot_ptr(word * usize::BITS as usize + bit) };
                let header = object as *const T as *const GcHeader;
                let payload = payload_layout(object);
                add(
                    &mut out.known_text_property_capacity_bytes,
                    text_property_storage(header).known_owned_capacity_bytes,
                );
                let shared = shared_bytecode_bytes(header);
                add(
                    &mut out.known_owned_payload_capacity_bytes,
                    payload.capacity_bytes.saturating_sub(shared),
                );
                add(&mut out.known_payload_logical_bytes, payload.logical_bytes);
                add(&mut out.bytecode_shared_reference_bytes, shared);
                note_header(
                    heap,
                    header,
                    size_of::<T>(),
                    payload,
                    marks_final,
                    full_marks,
                    ages,
                    old_dead_pacing,
                    old_dead_storage,
                );
            }
        }
    }
    out.occupied_slot_bytes = out.occupied_slots.saturating_mul(T::SLOT_BYTES);
    out.empty_backing_bytes = out.empty_pages.saturating_mul(OBJECT_PAGE_BYTES);
    out.page_tail_bytes = out
        .active_pages
        .saturating_mul(OBJECT_PAGE_BYTES - ObjectPage::<T>::SLOTS * T::SLOT_BYTES);
    out
}

fn object_struct_bytes(header: *const GcHeader) -> usize {
    unsafe {
        match (*header).kind {
            HeapObjectKind::String => size_of::<StringObj>(),
            HeapObjectKind::Float => size_of::<FloatObj>(),
            HeapObjectKind::VecLike => match (*(header as *const VecLikeHeader)).type_tag {
                VecLikeType::Vector => size_of::<VectorObj>(),
                VecLikeType::CharTable => size_of::<CharTableObj>(),
                VecLikeType::SubCharTable => size_of::<SubCharTableObj>(),
                VecLikeType::HashTable => size_of::<HashTableObj>(),
                VecLikeType::Obarray => size_of::<ObarrayObj>(),
                VecLikeType::Lambda => size_of::<LambdaObj>(),
                VecLikeType::Macro => size_of::<MacroObj>(),
                VecLikeType::ByteCode => size_of::<ByteCodeObj>(),
                VecLikeType::Record | VecLikeType::WindowConfiguration => size_of::<RecordObj>(),
                VecLikeType::Font => size_of::<FontObj>(),
                VecLikeType::Overlay => size_of::<OverlayObj>(),
                VecLikeType::Marker => size_of::<MarkerObj>(),
                VecLikeType::Buffer => size_of::<BufferObj>(),
                VecLikeType::Window => size_of::<WindowObj>(),
                VecLikeType::Frame => size_of::<FrameObj>(),
                VecLikeType::Timer => size_of::<TimerObj>(),
                VecLikeType::Process => size_of::<ProcessObj>(),
                VecLikeType::Terminal => size_of::<TerminalObj>(),
                VecLikeType::Xwidget => size_of::<XwidgetObj>(),
                VecLikeType::XwidgetView => size_of::<XwidgetViewObj>(),
                VecLikeType::SurfaceHandle => size_of::<SurfaceObj>(),
                VecLikeType::VideoHandle => size_of::<VideoObj>(),
                VecLikeType::BoolVector => size_of::<BoolVectorObj>(),
                VecLikeType::Subr => size_of::<SubrObj>(),
                VecLikeType::Bignum => size_of::<BignumObj>(),
                VecLikeType::SymbolWithPos => size_of::<SymbolWithPosObj>(),
                VecLikeType::Finalizer => size_of::<FinalizerObj>(),
                VecLikeType::Sqlite => size_of::<SqliteObj>(),
                VecLikeType::Thread | VecLikeType::Mutex | VecLikeType::CondVar => {
                    size_of::<ThreadingHandleObj>()
                }
                VecLikeType::UserPtr => size_of::<UserPtrObj>(),
                VecLikeType::ModuleFunction => size_of::<ModuleFunctionObj>(),
            },
        }
    }
}

fn header_payload(header: *const GcHeader) -> PayloadLayout {
    unsafe {
        match (*header).kind {
            HeapObjectKind::String => {
                string_payload_with_text_properties(&(*(header as *const StringObj)).data)
            }
            HeapObjectKind::Float => PayloadLayout::default(),
            HeapObjectKind::VecLike => {
                let v = header as *const VecLikeHeader;
                match (*v).type_tag {
                    VecLikeType::ByteCode => {
                        bytecode_payload_with_text_properties(&*(v as *const ByteCodeObj))
                    }
                    VecLikeType::HashTable => PayloadLayout {
                        capacity_bytes: (*(v as *const HashTableObj))
                            .table
                            .data
                            .known_storage_bytes(),
                        ..PayloadLayout::default()
                    },
                    VecLikeType::Bignum => {
                        TaggedHeap::bignum_payload_layout(&(*(v as *const BignumObj)).value)
                    }
                    VecLikeType::BoolVector => {
                        let bytes = size_of_val((*(v as *const BoolVectorObj)).words());
                        PayloadLayout {
                            logical_bytes: bytes,
                            capacity_bytes: bytes,
                            owned: true,
                            mapped: false,
                        }
                    }
                    VecLikeType::Font => {
                        let mut payload = TaggedHeap::veclike_payload_layout(v);
                        payload.capacity_bytes = TaggedHeap::object_bytes_from_header(header)
                            .saturating_sub(size_of::<FontObj>());
                        payload
                    }
                    _ => TaggedHeap::veclike_payload_layout(v),
                }
            }
        }
    }
}

fn boxed_inventory(
    heap: &TaggedHeap,
    mut header: *const GcHeader,
    marks_final: bool,
    full_marks: bool,
    out: &mut MemoryInventory,
) {
    while !header.is_null() {
        let class = TaggedHeap::boxed_class(header);
        let index = match out.boxed.iter().position(|item| item.class == class) {
            Some(index) => index,
            None => {
                out.boxed.push(BoxInventory {
                    class,
                    ..BoxInventory::default()
                });
                out.boxed.len() - 1
            }
        };
        let payload = header_payload(header);
        let struct_bytes = object_struct_bytes(header);
        let item = &mut out.boxed[index];
        add(
            &mut item.known_text_property_capacity_bytes,
            text_property_storage(header).known_owned_capacity_bytes,
        );
        item.objects += 1;
        add(&mut item.object_struct_bytes, struct_bytes);
        add(
            &mut item.pacing_bytes,
            TaggedHeap::object_bytes_from_header(header),
        );
        add(
            &mut item.known_owned_payload_capacity_bytes,
            payload
                .capacity_bytes
                .saturating_sub(shared_bytecode_bytes(header)),
        );
        note_header(
            heap,
            header,
            struct_bytes,
            payload,
            marks_final,
            full_marks,
            &mut out.ages,
            &mut out.old_dead_pacing_bytes,
            &mut out.old_dead_object_and_payload_bytes,
        );
        header = unsafe { (*header).gc_link() };
    }
}

fn mapped_inventory(heap: &TaggedHeap, marks_final: bool) -> MappedInventory {
    let mut out = MappedInventory {
        strings: heap.mapped_string_objects.len(),
        veclikes: heap.mapped_veclike_objects.len(),
        marked_live_image_object_bytes: marks_final.then_some(0),
        ..MappedInventory::default()
    };
    for range in &heap.mapped_cons_ranges {
        out.conses += range.len;
        add(
            &mut out.image_object_bytes,
            range.len * size_of::<ConsCell>(),
        );
        if let Some(live) = out.marked_live_image_object_bytes.as_mut() {
            add(live, range.live_count() * size_of::<ConsCell>());
        }
    }
    for range in &heap.mapped_float_ranges {
        out.floats += range.len;
        add(
            &mut out.image_object_bytes,
            range.len * size_of::<FloatObj>(),
        );
        if let Some(live) = out.marked_live_image_object_bytes.as_mut() {
            add(live, range.live_count() * size_of::<FloatObj>());
        }
    }
    for object in &heap.mapped_string_objects {
        add(&mut out.image_object_bytes, object.byte_len);
        let payload = unsafe { string_payload_with_text_properties(&(*object.ptr).data) };
        let text_properties = text_property_storage(object.ptr.cast());
        add(
            &mut out.known_text_property_capacity_bytes,
            text_properties.known_owned_capacity_bytes,
        );
        out.text_properties.merge(&text_properties);
        add(
            &mut out.copied_owned_payload_capacity_bytes,
            payload.capacity_bytes,
        );
        if object.marked
            && let Some(live) = out.marked_live_image_object_bytes.as_mut()
        {
            add(live, object.byte_len);
        }
    }
    for object in &heap.mapped_veclike_objects {
        add(&mut out.image_object_bytes, object.byte_len);
        let header = object.header.cast::<GcHeader>();
        let payload = header_payload(header);
        let text_properties = text_property_storage(header);
        add(
            &mut out.known_text_property_capacity_bytes,
            text_properties.known_owned_capacity_bytes,
        );
        out.text_properties.merge(&text_properties);
        let shared = shared_bytecode_bytes(header);
        add(
            &mut out.copied_owned_payload_capacity_bytes,
            payload.capacity_bytes.saturating_sub(shared),
        );
        add(&mut out.bytecode_shared_reference_bytes, shared);
        if object.marked
            && let Some(live) = out.marked_live_image_object_bytes.as_mut()
        {
            add(live, object.byte_len);
        }
    }
    out
}

fn vector_buffer<T>(name: &'static str, buffer: &Vec<T>) -> BufferInventory {
    BufferInventory {
        name,
        elements: buffer.len(),
        element_bytes: size_of::<T>(),
        logical_bytes: buffer.len().saturating_mul(size_of::<T>()),
        capacity_elements: buffer.capacity(),
        capacity_bytes: buffer.capacity().saturating_mul(size_of::<T>()),
        hash_capacity_entries: None,
    }
}

fn hash_buffer<T>(name: &'static str, buffer: &FxHashSet<T>) -> BufferInventory {
    BufferInventory {
        name,
        elements: buffer.len(),
        element_bytes: size_of::<T>(),
        logical_bytes: buffer.len().saturating_mul(size_of::<T>()),
        hash_capacity_entries: Some(buffer.capacity()),
        ..BufferInventory::default()
    }
}

fn buffer_inventory(heap: &TaggedHeap) -> Vec<BufferInventory> {
    let mut out = vec![
        vector_buffer("collector-r-seed", &heap.generational.r_seed),
        vector_buffer("collector-promotion", &heap.generational.promo),
        vector_buffer("collector-gray", &heap.gray_queue),
        vector_buffer("write-records", &heap.dirty_writes),
        hash_buffer("persistent-mapped-remset", &heap.mapped_remembered),
        hash_buffer("satb-snapshotted-owners", &heap.satb_snapshotted_owners),
        hash_buffer("satb-string-preimages", &heap.satb_string_preimage_addrs),
        hash_buffer("concurrent-cloned-vectors", &heap.concurrent_cloned_vectors),
        vector_buffer(
            "retired-vector-buffer-descriptors",
            &heap.retired_vector_buffers,
        ),
        vector_buffer("weak-table-registry", &heap.weak_hash_tables),
        vector_buffer(
            "permanent-weak-table-registry",
            &heap.permanent_weak_hash_tables,
        ),
        vector_buffer("finalizer-registry", &heap.finalizer_registry),
        vector_buffer("doomed-finalizers", &heap.doomed_finalizer_functions),
    ];
    let mut retired = BufferInventory {
        name: "retired-vector-payloads",
        element_bytes: size_of::<TaggedValue>(),
        ..BufferInventory::default()
    };
    for buffer in &heap.retired_vector_buffers {
        retired.elements += buffer.len();
        retired.capacity_elements += buffer.capacity();
    }
    retired.logical_bytes = retired.elements.saturating_mul(retired.element_bytes);
    retired.capacity_bytes = retired
        .capacity_elements
        .saturating_mul(retired.element_bytes);
    out.push(retired);
    out.push(vector_buffer(
        "shared-satb",
        &heap.satb_shared.lock().unwrap(),
    ));
    out.push(vector_buffer(
        "shared-deferred",
        &heap.deferred_veclikes.lock().unwrap(),
    ));
    for mutator in heap.mutators() {
        out.extend([
            vector_buffer("mutator-remset", &mutator.remset),
            hash_buffer("mutator-mapped-remset", &mutator.r_mapped_seen),
            vector_buffer("mutator-birth-headers", &mutator.black_born),
            vector_buffer("mutator-birth-regions", &mutator.black_born_regions),
            vector_buffer("mutator-major-cons-writes", &mutator.major_cons_writes),
            vector_buffer("mutator-symbol-preimages", &mutator.major_symbol_preimages),
        ]);
    }
    out
}

#[derive(Debug, Default, Serialize)]
pub(super) struct TextPropertyInventory {
    pub(super) tables: usize,
    pub(super) table_struct_bytes: usize,
    pub(super) node_logical_bytes: usize,
    pub(super) node_capacity_bytes: usize,
    pub(super) cached_range_capacity_bytes: usize,
    pub(super) cached_range_unavailable_tables: usize,
    pub(super) property_name_reference_entries: usize,
    pub(super) property_name_reference_capacity_entries: usize,
    pub(super) shared_property_name_tables: usize,
    pub(super) known_owned_capacity_bytes: usize,
}

impl TextPropertyInventory {
    fn merge(&mut self, other: &Self) {
        add(&mut self.tables, other.tables);
        add(&mut self.table_struct_bytes, other.table_struct_bytes);
        add(&mut self.node_logical_bytes, other.node_logical_bytes);
        add(&mut self.node_capacity_bytes, other.node_capacity_bytes);
        add(
            &mut self.cached_range_capacity_bytes,
            other.cached_range_capacity_bytes,
        );
        add(
            &mut self.cached_range_unavailable_tables,
            other.cached_range_unavailable_tables,
        );
        add(
            &mut self.property_name_reference_entries,
            other.property_name_reference_entries,
        );
        add(
            &mut self.property_name_reference_capacity_entries,
            other.property_name_reference_capacity_entries,
        );
        add(
            &mut self.shared_property_name_tables,
            other.shared_property_name_tables,
        );
        add(
            &mut self.known_owned_capacity_bytes,
            other.known_owned_capacity_bytes,
        );
    }
    fn note(&mut self, string: &crate::heap_types::LispString) {
        if !string.has_intervals() {
            return;
        }
        let memory = string.intervals().memory_telemetry_storage();
        self.tables += 1;
        add(&mut self.table_struct_bytes, memory.table_struct_bytes);
        add(&mut self.node_logical_bytes, memory.node_logical_bytes);
        add(&mut self.node_capacity_bytes, memory.node_capacity_bytes);
        add(
            &mut self.cached_range_capacity_bytes,
            memory.cached_range_capacity_bytes.unwrap_or(0),
        );
        self.cached_range_unavailable_tables +=
            usize::from(memory.cached_range_capacity_bytes.is_none());
        add(
            &mut self.property_name_reference_entries,
            memory.shared_property_names,
        );
        add(
            &mut self.property_name_reference_capacity_entries,
            memory.shared_property_name_capacity_entries,
        );
        self.shared_property_name_tables += usize::from(memory.shared_property_name_owners > 1);
        add(
            &mut self.known_owned_capacity_bytes,
            memory.known_boxed_owned_capacity_bytes(),
        );
    }
}

fn text_property_storage(header: *const GcHeader) -> TextPropertyInventory {
    let mut out = TextPropertyInventory::default();
    unsafe {
        match (*header).kind {
            HeapObjectKind::String => out.note(&(*(header as *const StringObj)).data),
            HeapObjectKind::VecLike
                if (*(header as *const VecLikeHeader)).type_tag == VecLikeType::ByteCode =>
            {
                if let Some(docstring) = &(*(header as *const ByteCodeObj)).data.docstring {
                    out.note(docstring);
                }
            }
            _ => {}
        }
    }
    out
}

fn string_payload_with_text_properties(string: &crate::heap_types::LispString) -> PayloadLayout {
    let mut payload = TaggedHeap::string_payload_layout(string);
    if string.has_intervals() {
        let memory = string.intervals().memory_telemetry_storage();
        payload.capacity_bytes = payload
            .capacity_bytes
            .saturating_add(memory.known_boxed_owned_capacity_bytes());
        // Logical interval node bytes are distinct from retained Vec capacity.
        payload.logical_bytes = payload
            .logical_bytes
            .saturating_add(memory.table_struct_bytes)
            .saturating_add(memory.node_logical_bytes)
            .saturating_add(memory.cached_range_logical_bytes.unwrap_or(0));
        payload.owned = true;
    }
    payload
}

fn bytecode_payload_with_text_properties(object: &ByteCodeObj) -> PayloadLayout {
    let mut payload = TaggedHeap::bytecode_payload_layout(object);
    if let Some(docstring) = &object.data.docstring {
        if docstring.has_intervals() {
            let memory = docstring.intervals().memory_telemetry_storage();
            payload.capacity_bytes = payload
                .capacity_bytes
                .saturating_add(memory.known_boxed_owned_capacity_bytes());
            payload.logical_bytes = payload
                .logical_bytes
                .saturating_add(memory.table_struct_bytes)
                .saturating_add(memory.node_logical_bytes)
                .saturating_add(memory.cached_range_logical_bytes.unwrap_or(0));
            payload.owned = true;
        }
    }
    payload
}

#[cfg(test)]
#[path = "tests/memory_inventory_test.rs"]
mod tests;
