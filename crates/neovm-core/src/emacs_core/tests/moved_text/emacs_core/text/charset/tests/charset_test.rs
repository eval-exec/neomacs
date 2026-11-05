use crate::emacs_core::eval::Context;

use crate::emacs_core::pdump::dump_to_file;

use neovm_text::emacs_core::charset::*;

use neovm_value::error::{FlowKind, FlowRef, FlowResultExt as _};

use neovm_value::intern::{intern, resolve_sym};

#[test]
fn charset_registry_plist_values_survive_exact_gc_and_pdump_dump() {
    crate::test_utils::init_test_tracing();
    reset_charset_registry();

    let mut eval = Context::new();

    let mut args = vec![Value::NIL; 17];
    args[0] = Value::symbol("charset-pdump-root-test");
    args[1] = Value::fixnum(1);
    args[2] = Value::vector(vec![Value::fixnum(0), Value::fixnum(127)]);
    args[16] = Value::list(vec![
        Value::symbol("doc"),
        Value::string("charset plist string should stay live"),
    ]);
    builtin_define_charset_internal(args).unwrap();

    eval.gc_collect_exact();

    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("charset-registry-root-test.pdump");
    dump_to_file(&eval, &dump_path).expect("pdump dump should keep charset plist strings alive");

    reset_charset_registry();
}

#[test]
fn find_charset_region_ascii_default() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert(&"a".repeat(100));
    }
    let r =
        builtin_find_charset_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(100)]).unwrap();
    let items = list_to_vec(&r).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].is_bare_symbol_named("ascii"));
}

#[test]
fn find_charset_region_with_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert(&"a".repeat(100));
    }
    let r = builtin_find_charset_region(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(100), Value::NIL],
    )
    .unwrap();
    let items = list_to_vec(&r).unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn find_charset_region_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert!(builtin_find_charset_region(&mut eval, vec![Value::fixnum(1)]).is_err());
    assert!(
        builtin_find_charset_region(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(2), Value::NIL, Value::NIL,]
        )
        .is_err()
    );
}

#[test]
fn find_charset_region_eval_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert("aé😀");
    }

    // GNU never surfaces the internal `unicode-bmp` subset charset: both the
    // BMP é and the astral 😀 canonicalize to the dimension-3 `unicode`
    // charset, so the deduplicated list is just (ascii unicode).
    let all = builtin_find_charset_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(4)])
        .expect("find-charset-region all");
    assert_eq!(
        all,
        Value::list(vec![Value::symbol("ascii"), Value::symbol("unicode"),])
    );

    let bmp = builtin_find_charset_region(&mut eval, vec![Value::fixnum(2), Value::fixnum(3)])
        .expect("find-charset-region bmp");
    assert_eq!(bmp, Value::list(vec![Value::symbol("unicode")]));

    let empty = builtin_find_charset_region(&mut eval, vec![Value::fixnum(4), Value::fixnum(4)])
        .expect("find-charset-region empty");
    assert_eq!(empty, Value::list(vec![Value::symbol("ascii")]));
}

#[test]
fn find_charset_region_eval_out_of_range_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert("abc");
    }
    let current = Value::make_buffer(
        eval.edit
            .buffers
            .current_buffer()
            .expect("current buffer")
            .id,
    );
    for (beg, end) in [(0, 2), (1, 5)] {
        match builtin_find_charset_region(&mut eval, vec![Value::fixnum(beg), Value::fixnum(end)])
            .kinded()
        {
            Err(FlowKind::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "args-out-of-range");
                assert_eq!(
                    sig.data,
                    vec![current, Value::fixnum(beg), Value::fixnum(end)]
                );
            }
            other => panic!("expected args-out-of-range signal, got {other:?}"),
        }
    }
}

#[test]
fn charset_after_default_returns_unicode() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert("😀");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }
    let r = builtin_charset_after(&mut eval, vec![]).unwrap();
    assert!(r.is_bare_symbol_named("unicode"));
}

#[test]
fn charset_after_with_pos() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert("😀");
    }
    let r = builtin_charset_after(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(r.is_bare_symbol_named("unicode"));
}

#[test]
fn charset_after_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert!(builtin_charset_after(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).is_err());
}

#[test]
fn charset_after_eval_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert("aé😀");
    }

    // No arg uses char after point; after insert point is at EOB.
    assert!(
        builtin_charset_after(&mut eval, vec![])
            .expect("charset-after no arg")
            .is_nil()
    );

    assert_eq!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(1)]).expect("pos 1"),
        Value::symbol("ascii")
    );
    // GNU `charset-after` returns `CHAR_CHARSET`, which maps both the BMP é and
    // the astral 😀 to the dimension-3 `unicode` charset (never `unicode-bmp`).
    assert_eq!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(2)]).expect("pos 2"),
        Value::symbol("unicode")
    );
    assert_eq!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(3)]).expect("pos 3"),
        Value::symbol("unicode")
    );
    assert!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(4)])
            .expect("pos 4")
            .is_nil()
    );
    assert!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(0)])
            .expect("pos 0")
            .is_nil()
    );
    assert!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(10)])
            .expect("pos 10")
            .is_nil()
    );
    assert!(builtin_charset_after(&mut eval, vec![Value::string("x")]).is_err());
}

#[test]
fn charset_after_respects_narrowed_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval
            .edit
            .buffers
            .current_buffer_mut()
            .expect("current buffer must exist");
        buf.insert("abé");
        buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(0, 2));
    }

    assert_eq!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(2)]).expect("pos 2"),
        Value::symbol("ascii")
    );
    assert!(
        builtin_charset_after(&mut eval, vec![Value::fixnum(3)])
            .expect("pos 3 at ZV")
            .is_nil()
    );
}

/// A [`CharsetDecoder`] resolved once must answer EXACTLY what the
/// per-character registry path answers, for every registered charset and
/// across each one's whole code range.
///
/// This is the only thing standing between "resolve once" and a silently
/// different decoding: the fast body reimplements `decode_char`'s order (ASCII
/// short-circuit, then range, then map) for the one shape it claims, and a
/// reordering there would still compile and would still decode most text
/// correctly. Charsets the fast body declines -- `Offset`, `Subset`,
/// `Superset`, and any unified charset -- are swept too, because they must
/// keep going through the registry rather than silently acquiring a fast body.
#[test]
fn a_resolved_charset_decoder_answers_exactly_like_the_registry() {
    crate::test_utils::init_test_tracing();
    // The charsets are defined during startup, so a bare registry holds only
    // the handful built in. Boot the image first, in THIS thread, because the
    // registry is thread-local.
    let _booted = crate::test_utils::runtime_startup_context();
    let mut swept = 0usize;
    let mut charsets = 0usize;
    for charset in super::registered_charset_names() {
        let Some((min_code, max_code, dimension)) = super::charset_code_bounds(charset) else {
            continue;
        };
        let Some(decoder) = super::charset_decoder(charset) else {
            continue;
        };
        assert_eq!(decoder.dimension(), dimension, "dimension for {charset:?}");
        charsets += 1;

        // The whole range where it is small, otherwise both ends plus a stride
        // through the middle, so a map hole cannot hide between samples.
        let span = max_code.saturating_sub(min_code);
        let step = if span <= 4096 { 1 } else { span / 4096 };
        let mut code = min_code;
        while code <= max_code {
            let bytes: Vec<u8> = (0..dimension)
                .rev()
                .map(|i| ((code >> (8 * i)) & 0xff) as u8)
                .collect();
            assert_eq!(
                decoder.decode_from_bytes(&bytes),
                super::charset_decode_char_from_bytes(charset, &bytes),
                "{charset:?} disagreed at code point {code:#x}"
            );
            swept += 1;
            code = code.saturating_add(step.max(1));
        }
    }
    assert!(
        charsets > 100,
        "expected the image's charsets, saw {charsets}"
    );
    assert!(
        swept > 100_000,
        "sweep was too small to mean anything: {swept}"
    );
}

#[cfg(test)]
#[path = "prepared_mule_test.rs"]
mod prepared_mule;

#[cfg(test)]
#[path = "mule_identity_test.rs"]
mod mule_identity;
