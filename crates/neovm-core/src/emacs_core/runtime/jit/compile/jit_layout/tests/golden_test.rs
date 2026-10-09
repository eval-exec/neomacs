//! Golden tests of `jit_layout` (p1-0-integration §3.3/§7 X6, P1.1 T13/T14,
//! P1.4's layout tests, P2.1 correction 5): every probe succeeds on this
//! build, and what generated code would write or read through it is exactly
//! what the Rust side writes or reads.

use super::heap::*;
use super::*;
use crate::buffer::BufferId;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::{SavedBindingValue, SavedBufferId};
use crate::emacs_core::value::LambdaParams;

/// The word at `base + offset`.
fn words_at(base: *const u8, offset: usize) -> usize {
    // SAFETY: callers pass offsets the probes or `offset_of!` proved lie
    // inside the object at `base`.
    unsafe { base.add(offset).cast::<usize>().read_unaligned() }
}

/// An aligned atomic forwarder word at its pinned JIT offset.
fn atomic_word_at(base: *const u8, offset: usize) -> usize {
    // SAFETY: callers pass a live descriptor's pinned, aligned AtomicValue
    // field; repr(transparent) puts its AtomicUsize at offset zero.
    unsafe {
        (&*base.add(offset).cast::<std::sync::atomic::AtomicUsize>())
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Inspect initialized fields only. A typed enum's padding is never an
/// initialized integer image, even if its destination was zeroed first.
fn entry_words(entry: &SpecBinding) -> [u64; ENTRY_WORDS] {
    let tag = SpecBindingTag::from(entry);
    let frames = backtrace_layout().expect("fixed frame records");
    let binds = let_layout();
    let mut words = [0u64; ENTRY_WORDS];
    words[0] = tag as u64;
    let (template, small, fields): (EntryTemplate, u32, Vec<(*const u8, u64)>) = match entry {
        SpecBinding::Let { sym_id, old_value } => (
            binds.let_,
            sym_id.0,
            vec![(
                std::ptr::from_ref(old_value).cast(),
                old_value.as_plain().bits() as u64,
            )],
        ),
        SpecBinding::LetLocal {
            sym_id,
            old_value,
            buffer_id,
        } => (
            binds.let_local,
            sym_id.0,
            vec![
                (
                    std::ptr::from_ref(old_value).cast(),
                    old_value.bits() as u64,
                ),
                (std::ptr::from_ref(buffer_id).cast(), buffer_id.0),
            ],
        ),
        SpecBinding::LetDefault {
            sym_id,
            old_value,
            buffer_id,
        } => (
            binds.let_default,
            sym_id.0,
            vec![
                (
                    std::ptr::from_ref(old_value).cast(),
                    old_value.as_plain().bits() as u64,
                ),
                (
                    std::ptr::from_ref(buffer_id).cast(),
                    buffer_id.get().map_or(0, |b| b.0),
                ),
            ],
        ),
        SpecBinding::Backtrace1 {
            function,
            arg,
            debug_on_exit,
        } => (
            frames.bt1,
            u32::from(*debug_on_exit),
            vec![
                (std::ptr::from_ref(function).cast(), function.bits() as u64),
                (std::ptr::from_ref(arg).cast(), arg.bits() as u64),
            ],
        ),
        SpecBinding::Backtrace2 {
            function,
            arg0,
            arg1,
        } => (
            frames.bt2,
            0,
            vec![
                (std::ptr::from_ref(function).cast(), function.bits() as u64),
                (std::ptr::from_ref(arg0).cast(), arg0.bits() as u64),
                (std::ptr::from_ref(arg1).cast(), arg1.bits() as u64),
            ],
        ),
        SpecBinding::BacktraceNative {
            function,
            args_ptr,
            nargs,
        } => (
            frames.native,
            *nargs,
            vec![
                (std::ptr::from_ref(function).cast(), function.bits() as u64),
                (
                    std::ptr::from_ref(args_ptr).cast(),
                    *args_ptr as usize as u64,
                ),
            ],
        ),
        // Debugger promotion changes the tag to an owned Backtrace. Its
        // header must refuse the lean template; no payload read is needed.
        _ => return words,
    };
    let small_at: Option<(*const u8, usize)> = match entry {
        SpecBinding::Let { sym_id, .. }
        | SpecBinding::LetLocal { sym_id, .. }
        | SpecBinding::LetDefault { sym_id, .. } => Some((
            std::ptr::from_ref(sym_id).cast(),
            size_of::<crate::emacs_core::intern::SymId>(),
        )),
        SpecBinding::Backtrace1 { debug_on_exit, .. } => {
            Some((std::ptr::from_ref(debug_on_exit).cast(), size_of::<bool>()))
        }
        SpecBinding::BacktraceNative { nargs, .. } => {
            Some((std::ptr::from_ref(nargs).cast(), size_of::<u32>()))
        }
        _ => None,
    };
    let base = std::ptr::from_ref(entry).cast::<u8>() as usize;
    if let Some((address, size)) = small_at {
        let offset = address as usize - base;
        assert_eq!(template.small_shift, Some((offset * 8) as u32));
        assert_eq!(
            template.header_mask,
            u8::MAX as u64 | (((1u64 << (size * 8)) - 1) << (offset * 8))
        );
    }
    // SAFETY: repr(u8) guarantees an initialized tag byte at offset zero.
    assert_eq!(
        unsafe { std::ptr::from_ref(entry).cast::<u8>().read() },
        tag as u8
    );
    assert_eq!(template.header, tag as u64);
    words[0] = template.header_with(small);
    for (i, &(address, bits)) in fields.iter().enumerate() {
        assert_eq!(address as usize - base, template.field(i) as usize);
        words[template.field(i) as usize / WORD] = bits;
    }
    words
}

#[test]
fn vec_offsets_read_the_specpdl_and_bind_stack_through_reallocation() {
    let mut ev = Context::new();
    let spec = specpdl_vec_offsets().expect("the specpdl Vec probe succeeds");
    let binds = jit_bind_stack_vec_offsets().expect("the bind-stack Vec probe succeeds");
    let base = std::ptr::from_ref(&ev).cast::<u8>();
    for round in 0..3 {
        let at = CONTEXT_SPECPDL_OFFSET;
        assert_eq!(words_at(base, at + spec.ptr), ev.specpdl.as_ptr() as usize);
        assert_eq!(words_at(base, at + spec.len), ev.specpdl.len());
        assert_eq!(words_at(base, at + spec.cap), ev.specpdl.capacity());
        let at = CONTEXT_JIT_BIND_STACK_OFFSET;
        assert_eq!(
            words_at(base, at + binds.ptr),
            ev.jit_bind_stack.as_ptr() as usize
        );
        assert_eq!(words_at(base, at + binds.len), ev.jit_bind_stack.len());
        assert_eq!(words_at(base, at + binds.cap), ev.jit_bind_stack.capacity());
        // Grow both past their capacity: the data pointers move.
        let extra = ev.specpdl.capacity() + 17 * (round + 1);
        ev.specpdl.reserve(extra);
        ev.specpdl.push(SpecBinding::GcRoot { value: Value::NIL });
        ev.jit_bind_stack.reserve(ev.jit_bind_stack.capacity() + 9);
        ev.jit_bind_stack.push(round);
    }
    ev.specpdl.clear();
}

#[test]
fn the_generic_vec_probe_agrees_with_the_buffer_walk_probe() {
    let generic = vec_offsets_with(|| None::<Box<crate::buffer::buffer::Buffer>>)
        .expect("the generic probe succeeds");
    let (ptr, len) = buffer_walk::slots_vec_offsets().expect("the buffer probe succeeds");
    assert_eq!((generic.ptr, generic.len), (ptr, len));
}

#[test]
fn the_context_words_are_the_fields_they_name() {
    let mut ev = Context::new();
    ev.depth = 0x1234;
    ev.max_depth = 0x5678;
    let base = std::ptr::from_ref(&ev).cast::<u8>();
    assert_eq!(words_at(base, CONTEXT_DEPTH_OFFSET), 0x1234);
    assert_eq!(words_at(base, CONTEXT_MAX_DEPTH_OFFSET), 0x5678);
    assert_eq!(
        words_at(base, CONTEXT_OBARRAY_OFFSET + OBARRAY_FUNCTION_EPOCH_OFFSET) as u64,
        ev.obarray.function_epoch()
    );
    ev.depth = 0;
}

/// P1.1 T13: for every arity the speculated call path pushes, the entry the
/// shim writes matches its template, and the image generated code would
/// write instead decodes in Rust as the same frame -- through
/// `backtrace_entry_values` and the balanced pop.
#[test]
fn backtrace_templates_match_the_shim_and_decode_in_rust() {
    let layout = backtrace_layout().expect("the fixed backtrace records are supported");
    let mut ev = Context::new();
    let function = Value::symbol("neovm--jl-f");
    for nargs in 0..=6usize {
        let args: Vec<i64> = (0..nargs)
            .map(|i| Value::fixnum(100 + i as i64).bits() as i64)
            .collect();
        let (template, small) = layout.frame_for(nargs);
        // The shim's entry matches the template and holds the fields at its
        // offsets.
        // SAFETY: `args` outlives the frame, popped below.
        unsafe { ev.push_backtrace_frame_from_native_args(function, args.as_ptr(), nargs) };
        let words = entry_words(&ev.specpdl[0]);
        assert!(template.matches(&words, small), "nargs {nargs}: shim entry");
        let field = |i: usize| words[template.field(i) as usize / WORD];
        assert_eq!(field(0), function.bits() as u64);
        match nargs {
            1 => assert_eq!(field(1), args[0] as u64),
            2 => assert_eq!([field(1), field(2)], [args[0] as u64, args[1] as u64]),
            _ => assert_eq!(field(1), args.as_ptr() as u64),
        }
        assert!(ev.pop_native_backtrace_frame(0));
        // The JIT-shaped image, written into spare capacity the way
        // generated code writes it, decodes as the same frame.
        let fields: Vec<u64> = match nargs {
            1 => vec![function.bits() as u64, args[0] as u64],
            2 => vec![function.bits() as u64, args[0] as u64, args[1] as u64],
            _ => vec![function.bits() as u64, args.as_ptr() as u64],
        };
        let image = template.image(small, &fields);
        ev.specpdl.reserve(1);
        // SAFETY: spare capacity for one entry. The fixed tag and record
        // offsets place valid Value words and the live args pointer/count
        // in the chosen variant, written before the length covers it.
        unsafe {
            ev.specpdl
                .as_mut_ptr()
                .cast::<[u64; ENTRY_WORDS]>()
                .write(image);
            ev.specpdl.set_len(1);
        }
        let (f, a, debug, unevalled) = ev
            .backtrace_entry_values(&ev.specpdl[0])
            .expect("the JIT frame is inspectable");
        assert_eq!(f, function);
        let want: Vec<Value> = args.iter().map(|&b| Value::from_bits(b as usize)).collect();
        assert_eq!(a.as_slice(), want.as_slice(), "nargs {nargs}");
        assert!(!debug && !unevalled);
        assert!(
            ev.pop_native_backtrace_frame(0),
            "the balanced pop admits the JIT frame (nargs {nargs})"
        );
    }
}

/// A frame the debugger flagged no longer matches its template, so a direct
/// call's inline pop sends the return to the exit debugger.
#[test]
fn a_flagged_frame_does_not_match_its_template() {
    let layout = backtrace_layout().expect("fixed backtrace records");
    let mut ev = Context::new();
    let args: Vec<i64> = (0..3).map(|i| Value::fixnum(i).bits() as i64).collect();
    for nargs in [1usize, 2, 3] {
        let (template, small) = layout.frame_for(nargs);
        // SAFETY: `args` outlives the frame, unbound below.
        unsafe { ev.push_backtrace_frame_from_native_args(Value::NIL, args.as_ptr(), nargs) };
        assert!(template.matches(&entry_words(&ev.specpdl[0]), small));
        // `backtrace-debug`: a byte write (one argument) or a promotion to
        // the owned shape (two or more).
        assert!(ev.set_backtrace_debug_on_exit(0, true));
        assert!(
            !template.matches(&entry_words(&ev.specpdl[0]), small),
            "nargs {nargs}: a flagged frame must leave the inline pop"
        );
        ev.set_backtrace_debug_on_exit(0, false);
        let _ = ev.pop_bytecode_backtrace_frame_with_result(0, Ok(Value::NIL));
        assert!(ev.specpdl.is_empty());
    }
    // The three lean shapes are told apart by their headers alone.
    assert_ne!(layout.bt1.header, layout.bt2.header);
    assert_ne!(layout.bt2.header, layout.native.header_with(0));
}

/// P1.4's `let` templates: every record field agrees with its Rust-built entry.
#[test]
fn let_templates_match_rust_built_entries() {
    let layout = let_layout();
    let sym = crate::emacs_core::intern::intern("neovm--jl-let");
    let entry = SpecBinding::Let {
        sym_id: sym,
        old_value: SavedBindingValue::from_plain(Value::fixnum(7)),
    };
    let words = entry_words(&entry);
    assert!(layout.let_.matches(&words, sym.0));
    assert_eq!(
        words[layout.let_.field(0) as usize / WORD],
        Value::fixnum(7).bits() as u64
    );
    let local = SpecBinding::LetLocal {
        sym_id: sym,
        old_value: Value::fixnum(8),
        buffer_id: BufferId(3),
    };
    let words = entry_words(&local);
    assert!(layout.let_local.matches(&words, sym.0));
    assert!(!layout.let_.matches(&words, sym.0));
    assert_eq!(words[layout.let_local.field(1) as usize / WORD], 3);
    let default = SpecBinding::LetDefault {
        sym_id: sym,
        old_value: SavedBindingValue::from_plain(Value::fixnum(9)),
        buffer_id: SavedBufferId::from_option(Some(BufferId(4))),
    };
    let words = entry_words(&default);
    assert!(layout.let_default.matches(&words, sym.0));
    assert_eq!(words[layout.let_default.field(1) as usize / WORD], 4);
}

/// Exercise the full-word saved-value sentinel and the optional buffer's
/// zero niche through actual JIT-shaped images, including high symbol IDs.
#[test]
fn binding_record_images_preserve_unbound_values_and_optional_buffers() {
    let layout = let_layout();
    for sym_id in [0, 0x0012_3457, u32::MAX] {
        for old_value in [Value::NIL, Value::UNBOUND, Value::fixnum(-71)] {
            for buffer in [None, Some(BufferId(1)), Some(BufferId(u64::MAX))] {
                let entry = SpecBinding::LetDefault {
                    sym_id: crate::emacs_core::intern::SymId(sym_id),
                    old_value: crate::emacs_core::eval::SavedBindingValue::from_plain(old_value),
                    buffer_id: crate::emacs_core::eval::SavedBufferId::from_option(buffer),
                };
                let image = entry_words(&entry);
                assert!(layout.let_default.matches(&image, sym_id));
                // SAFETY: the fixed tag, initialized symbol/value words,
                // and valid zero-or-NonZero buffer bits form a LetDefault.
                let decoded = unsafe { &*image.as_ptr().cast::<SpecBinding>() };
                let SpecBinding::LetDefault {
                    sym_id: got_sym,
                    old_value: got_value,
                    buffer_id: got_buffer,
                } = decoded
                else {
                    panic!("the JIT image must decode as LetDefault");
                };
                assert_eq!(got_sym.0, sym_id);
                assert_eq!(got_value.as_plain(), old_value);
                assert_eq!(got_buffer.get(), buffer);
                assert_eq!(entry_words(decoded), image);
            }
        }
    }
}

#[test]
fn the_symbol_function_cell_is_the_word_compiled_code_reads() {
    let ev = Context::new();
    let sym = crate::emacs_core::intern::intern("car");
    let cell = ev.obarray.get_by_id(sym).expect("car is interned");
    let base = std::ptr::from_ref(cell).cast::<u8>();
    assert_eq!(
        words_at(base, LISP_SYMBOL_FUNCTION_OFFSET),
        ev.obarray
            .symbol_function_id(sym)
            .expect("car is fbound")
            .bits()
    );
}

fn tiny_function() -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::Return];
    f.constants = vec![Value::fixnum(41), Value::fixnum(42)].into();
    f.max_stack = 1;
    f.seal_hand_assembled_ops();
    Value::make_bytecode(f)
}

/// P2.1 correction 5: the runtime word a source guard compares is the
/// field's own bit pattern, the same for every function sharing the source.
#[test]
fn the_runtime_word_is_the_field_and_the_constant_base_is_the_pool() {
    let f = tiny_function();
    crate::emacs_core::eval::push_scratch_gc_root(f);
    let g = tiny_function();
    crate::emacs_core::eval::push_scratch_gc_root(g);
    let object = (f.bits() & !crate::tagged::value::TAG_MASK) as *const u8;
    let data = f.get_bytecode_data().expect("byte-code");
    assert_eq!(
        words_at(object, BYTECODE_RUNTIME_WORD_OFFSET),
        runtime_identity_word(data.jit_runtime())
    );
    let other = g.get_bytecode_data().expect("byte-code");
    assert_ne!(
        runtime_identity_word(data.jit_runtime()),
        runtime_identity_word(other.jit_runtime()),
        "two sources have two identities"
    );
    assert_eq!(
        runtime_identity_word(data.jit_runtime()),
        runtime_identity_word(&data.jit_runtime().clone()),
        "a shared handle has one identity"
    );
    let (ptr, len) = bytecode_constants_offsets().expect("the constant pool probe succeeds");
    assert_eq!(words_at(object, ptr), data.jit_constant_base() as usize);
    assert_eq!(words_at(object, len), 2);
}

#[test]
fn heap_words_are_where_compiled_code_reads_them() {
    let v = Value::make_float(2.5);
    let object = (v.bits() & !crate::tagged::value::TAG_MASK) as *const u8;
    // SAFETY: a live float object; its value word is at FLOAT_VALUE_OFFSET.
    let read = unsafe { object.add(FLOAT_VALUE_OFFSET as usize).cast::<f64>().read() };
    assert_eq!(read, 2.5);
    let bytes = [
        GcHeaderByte::Marked,
        GcHeaderByte::Kind,
        GcHeaderByte::Tenured,
        GcHeaderByte::Remembered,
        GcHeaderByte::TypeTag,
        GcHeaderByte::Flags,
        GcHeaderByte::Gen,
        GcHeaderByte::CollectionObserved,
    ];
    for (i, byte) in bytes.iter().enumerate() {
        assert_eq!(byte.offset(), i);
        assert_eq!(byte.is_reserved(), matches!(i, 4 | 5), "{byte:?}");
    }
    // A fresh young object's generation byte is live and zero; the reserved
    // bytes are zero.
    for byte in [
        GcHeaderByte::TypeTag,
        GcHeaderByte::Flags,
        GcHeaderByte::Gen,
        GcHeaderByte::CollectionObserved,
    ] {
        // SAFETY: a live float object starts with its 16-byte header.
        let b = unsafe { object.add(byte.offset()).read() };
        assert_eq!(b, 0, "{byte:?}");
    }
    // Byte 1 of a live header is its kind.
    // SAFETY: a live float object starts with its header.
    let kind = unsafe { object.add(GcHeaderByte::Kind.offset()).read() };
    assert_eq!(kind, crate::tagged::header::HeapObjectKind::Float as u8);
    assert!(value_vec_slice_offsets().is_some());
}

/// P1.4 Stage B: a symbol's baked cell, a buffer-local variable's cache
/// record, a forwarder's slot and the current buffer's raw id read, through
/// the offsets compiled code bakes, exactly what the Rust accessors read.
#[test]
fn variable_words_are_where_compiled_code_reads_them() {
    use crate::emacs_core::intern::intern;
    let mut ev = Context::new();
    ev.eval_str(
        "(progn (defvar neovm--jl-plain 42)
                (defvar neovm--jl-loc 1)
                (make-local-variable 'neovm--jl-loc)
                (setq neovm--jl-loc 2)
                neovm--jl-loc)",
    )
    .expect("fixture");
    // The cell.
    let plain = intern("neovm--jl-plain");
    let cell = ev.obarray.jit_symbol_cell_addr(plain).expect("in range") as *const u8;
    assert_eq!(
        words_at(cell, LISP_SYMBOL_VAL_OFFSET),
        Value::fixnum(42).bits()
    );
    assert!(
        ev.obarray
            .jit_symbol_cell_addr(crate::emacs_core::intern::SymId(u32::MAX))
            .is_none()
    );
    // The loaded BLV cache (read above, so loaded for this buffer).
    let loc = intern("neovm--jl-loc");
    let blv = ev.obarray.blv(loc).expect("buffer-local");
    let base = std::ptr::from_ref(blv).cast::<u8>();
    let buffer = ev.buffers.current_buffer_id().expect("a current buffer");
    assert_eq!(words_at(base, BLV_WHERE_BUF_ID_OFFSET), buffer.0 as usize);
    assert_eq!(words_at(base, BLV_VALCELL_OFFSET), blv.valcell.bits());
    assert_eq!(words_at(base, BLV_DEFCELL_OFFSET), blv.defcell.bits());
    assert_eq!(
        words_at(base, BLV_ALIST_EPOCH_OFFSET) as u64,
        blv.alist_epoch
    );
    assert_eq!(words_at(base, BLV_FWD_OFFSET), 0, "no forwarder");
    // SAFETY: the two bool bytes of a live record.
    let (local_if_set, found) = unsafe {
        (
            base.add(BLV_LOCAL_IF_SET_OFFSET).read(),
            base.add(BLV_FOUND_OFFSET).read(),
        )
    };
    assert_eq!((local_if_set, found), (u8::from(blv.local_if_set), 1));
    // SAFETY: a process static.
    let epoch = unsafe { (blv_alist_epoch_addr() as *const u64).read() };
    assert_eq!(epoch, blv.alist_epoch, "the cache is current");
    // The valcell's cdr, from the tagged cons word.
    let cdr = words_at(
        blv.valcell.bits() as *const u8,
        CONS_CDR_OFFSET - crate::tagged::value::TAG_CONS,
    );
    assert_eq!(cdr, Value::fixnum(2).bits());
    // The current buffer's raw id follows every switch, and a kill of the
    // current buffer leaves none.
    let ctx = std::ptr::from_ref(&ev).cast::<u8>();
    assert_eq!(
        words_at(ctx, CONTEXT_CURRENT_BUFFER_RAW_OFFSET),
        buffer.0 as usize
    );
    let other = ev.buffers.create_buffer("neovm--jl-other");
    ev.buffers.switch_current(other);
    let ctx = std::ptr::from_ref(&ev).cast::<u8>();
    assert_eq!(
        words_at(ctx, CONTEXT_CURRENT_BUFFER_RAW_OFFSET),
        other.0 as usize
    );
    ev.buffers.kill_buffer(other);
    assert_eq!(ev.buffers.current_buffer_id(), None);
    let ctx = std::ptr::from_ref(&ev).cast::<u8>();
    assert_eq!(words_at(ctx, CONTEXT_CURRENT_BUFFER_RAW_OFFSET), 0);
    // Forwarders' own slots.
    use crate::emacs_core::defvar_bool::ByteBooleanVars;
    ev.obarray
        .define_bool_variable("neovm--jl-bool", true, ByteBooleanVars::ErasedByLreadInit);
    ev.obarray.define_int_variable("neovm--jl-int", 7);
    let bool_fwd = ev.obarray.forwarder(intern("neovm--jl-bool")).expect("fwd");
    let int_fwd = ev.obarray.forwarder(intern("neovm--jl-int")).expect("fwd");
    let bool_base = std::ptr::from_ref(bool_fwd).cast::<u8>();
    // SAFETY: the offset is pinned to this live AtomicBool field.
    let bool_slot = unsafe {
        &*bool_base
            .add(LISP_BOOL_FWD_VALUE_OFFSET)
            .cast::<std::sync::atomic::AtomicBool>()
    };
    assert!(bool_slot.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(
        atomic_word_at(
            std::ptr::from_ref(int_fwd).cast(),
            LISP_INT_FWD_VALUE_OFFSET
        ),
        Value::fixnum(7).bits()
    );
    let obj = crate::emacs_core::forward::alloc_objfwd(Value::fixnum(9));
    assert_eq!(
        atomic_word_at(std::ptr::from_ref(obj).cast(), LISP_OBJ_FWD_VALUE_OFFSET),
        Value::fixnum(9).bits()
    );
    let kbd = crate::emacs_core::forward::alloc_kboard_objfwd(Value::fixnum(11));
    assert_eq!(
        atomic_word_at(
            std::ptr::from_ref(kbd).cast(),
            LISP_KBOARD_OBJ_FWD_VALUE_OFFSET
        ),
        Value::fixnum(11).bits()
    );
}
