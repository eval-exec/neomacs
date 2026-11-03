use super::LispString;
use crate::buffer::text_props::{
    EmptyTextProperties, reset_text_property_table_clones_for_test,
    text_property_table_clones_for_test,
};
use crate::buffer::{CharLen, CharPos0};
use crate::emacs_core::{Context, value::Value};

#[test]
fn empty_text_properties_can_cross_threads_without_a_table() {
    let empty = EmptyTextProperties::new();
    std::thread::spawn(move || {
        let view = empty.view();
        assert!(view.is_empty());
        assert!(view.as_table().is_none());
        assert_eq!(view.mutation_tick(), 0);
        assert!(
            view.object_interval_runs_for_char_len(CharLen::new(3))
                .is_empty()
        );
    })
    .join()
    .expect("empty property state is independent of a heap");
}

#[test]
fn interval_free_reads_never_attach_table_storage() {
    let string = LispString::from_utf8("abc");
    reset_text_property_table_clones_for_test();
    for _ in 0..8 {
        let view = string.intervals();
        assert!(view.as_table().is_none());
        assert!(view.is_empty());
        assert_eq!(view.mutation_tick(), 0);
        assert_eq!(
            view.get_property_at_char_pos(CharPos0::ZERO, Value::T),
            None
        );
        let mut roots = Vec::new();
        view.for_each_root(|value| roots.push(value));
        assert!(roots.is_empty());
    }
    assert!(!string.has_intervals());
    assert_eq!(text_property_table_clones_for_test(), 0);
}

#[test]
fn attached_empty_table_stays_distinct_from_absence() {
    let mut string = LispString::from_utf8("abc");
    let _ = string.intervals_mut();
    let view = string.intervals();
    assert!(view.is_empty());
    assert!(view.as_table().is_some());
    assert!(string.has_intervals());
    let cloned = string.clone();
    assert!(cloned.has_intervals());
    assert!(cloned.intervals().as_table().is_some());
}

#[test]
fn copying_a_view_does_not_clone_its_table() {
    let mut string = LispString::from_utf8("abc");
    let _ = string.intervals_mut();
    reset_text_property_table_clones_for_test();
    let view = string.intervals();
    let copy = view;
    assert!(std::ptr::eq(
        view.as_table().unwrap(),
        copy.as_table().unwrap(),
    ));
    assert_eq!(text_property_table_clones_for_test(), 0);
    let owned = copy.to_owned_table();
    assert!(owned.is_empty());
    assert_eq!(text_property_table_clones_for_test(), 1);
}

#[test]
fn clearing_an_attached_table_restores_the_empty_state() {
    let mut string = LispString::from_utf8("abc");
    let _ = string.intervals_mut();
    assert!(string.intervals().as_table().is_some());
    string.clear_intervals();
    assert!(!string.has_intervals());
    assert!(string.intervals().as_table().is_none());
}

#[test]
fn string_property_copies_and_empty_concat_keep_properties_and_revision_ticks() {
    let mut eval = Box::new(Context::new());
    eval.setup_thread_locals();
    {
        let face = Value::symbol("face");
        let mut string = LispString::from_utf8("abc");
        string.intervals_mut().put_property_in_char_range(
            crate::buffer::CharRange::from_usize(0, 3),
            face,
            Value::T,
        );
        let tick = string.intervals().mutation_tick();
        let syntax_tick = string.intervals().as_table().unwrap().syntax_prop_tick();

        let cloned = string.clone();
        assert_eq!(cloned.intervals().mutation_tick(), tick);
        assert_eq!(
            cloned
                .intervals()
                .get_property_at_char_pos(CharPos0::new(1), face),
            Some(Value::T),
        );

        let sliced = string.slice(1, 3).unwrap();
        assert_eq!(sliced.as_bytes(), b"bc");
        assert_eq!(
            sliced
                .intervals()
                .get_property_at_char_pos(CharPos0::ZERO, face),
            Some(Value::T),
        );

        let concatenated = string.concat(&LispString::from_utf8("x"));
        assert_eq!(concatenated.as_bytes(), b"abcx");
        assert_eq!(concatenated.intervals().mutation_tick(), tick + 1);
        assert_eq!(
            concatenated
                .intervals()
                .as_table()
                .unwrap()
                .syntax_prop_tick(),
            syntax_tick + 1,
        );
        assert_eq!(
            concatenated
                .intervals()
                .get_property_at_char_pos(CharPos0::new(2), face),
            Some(Value::T),
        );
        assert_eq!(
            concatenated
                .intervals()
                .get_property_at_char_pos(CharPos0::new(3), face),
            None,
        );
    }
    eval.shutdown().expect("finish owner-local property heap");
}
