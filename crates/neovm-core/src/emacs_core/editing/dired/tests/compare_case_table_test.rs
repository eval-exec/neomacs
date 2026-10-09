//! File completion compares decoded characters through the buffer case table.

use super::super::*;
use crate::emacs_core::{casetab, chartable};

fn context_with_mapping(mapped: u32, multibyte: bool) -> Context {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let standard = casetab::builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    let table = chartable::copy_char_table(standard).unwrap();
    let up = chartable::copy_char_table(table.as_char_table_obj().unwrap().extras.as_slice()[0])
        .unwrap();
    chartable::ct_set_single(&up, b'a' as i64, Value::fixnum(mapped as i64));
    table.with_char_table_mut(|obj| obj.extras.ensure_owned()[0] = up);
    casetab::builtin_set_case_table(&mut ctx, vec![table]).unwrap();
    let id = ctx.buffers.current_buffer_id().unwrap();
    ctx.buffers
        .set_buffer_multibyte_flag(id, multibyte)
        .unwrap();
    ctx
}

#[test]
fn file_completion_case_table_collects_decoded_custom_prefix() {
    let mut ctx = context_with_mapping(b'X' as u32, true);
    let dir = tempfile::tempdir().unwrap();
    for name in ["a-long", "X-long", "b-long"] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    let directory = LispString::from_utf8(dir.path().to_str().unwrap());
    let file = LispString::from_utf8("X");
    let names = collect_file_name_completions_with_parity(&mut ctx, &file, &directory, false, true)
        .unwrap();
    let mut names: Vec<_> = names.iter().map(|name| name.as_bytes().to_vec()).collect();
    names.sort();
    assert_eq!(names, vec![b"X-long".to_vec(), b"a-long".to_vec()]);
    let off = collect_file_name_completions_with_parity(&mut ctx, &file, &directory, false, false)
        .unwrap();
    assert_eq!(off.len(), 1);
    assert_eq!(off[0].as_bytes(), b"X-long");
}

#[test]
fn file_completion_case_table_unibyte_common_prefix_preserves_input_case() {
    let mut ctx = context_with_mapping(0x130, false);
    let file = LispString::from_utf8("0");
    let names = vec![
        LispString::from_utf8("a-one"),
        LispString::from_utf8("0-other"),
    ];
    let result =
        resolve_file_name_completion_with_parity(&mut ctx, &file, names.clone(), true).unwrap();
    assert_eq!(result.as_utf8_str(), Some("0-o"));
    let off = resolve_file_name_completion_with_parity(&mut ctx, &file, names, false).unwrap();
    assert_eq!(off.as_utf8_str(), Some(""));
}
