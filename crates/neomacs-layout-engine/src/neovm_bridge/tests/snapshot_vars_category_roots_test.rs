//! Pending snapshot operands must survive replacement plus callback GC.
//! Install as a child of `buffer_snapshot`, not the bridge's sibling tests.
use super::*;
use neovm_core::emacs_core::xdisp::{
    ProbeRow, RedisplayFontificationRequest, ensure_fontified_for_redisplay_for_window_request,
};

fn run_replacing_fontifier(
    eval: &mut neovm_core::emacs_core::eval::Context,
    buffer_id: neovm_core::buffer::BufferId,
) {
    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();
    ensure_fontified_for_redisplay_for_window_request(
        eval,
        buffer_id,
        1,
        2,
        RedisplayFontificationRequest::VisibleWalk,
        ProbeRow::None,
        1,
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("snapshot-root2-calls").unwrap().as_int(),
        Some(1),
        "the real VisibleWalk fontification callback must run exactly once"
    );
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "the replacing callback must complete an actual collection"
    );
}

fn assert_snapshot_values_alive(values: &[Value], owner: &str) {
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        for &value in values {
            // This raw-Value allocation oracle excludes freed cons cells,
            // even when their enclosing allocation block still survives.
            // Never inspect a captured payload before this check succeeds.
            assert!(
                heap.owns_heap_value_for_test(value),
                "{owner}: captured operand was reclaimed by callback GC"
            );
            assert!(
                heap.value_is_marked_for_test(value),
                "{owner}: captured operand was not marked by callback GC"
            );
        }
    });
}

fn assert_snapshot_values_reclaimed(values: &[Value], owner: &str) {
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        for &value in values {
            // Allocation/mark metadata only: the final owner has been dropped
            // and no Lisp query may touch these captured operands now.
            assert!(
                !heap.owns_heap_value_for_test(value),
                "{owner}: final snapshot drop must release captured operand"
            );
            // Mark queries treat unowned non-cons addresses as permanent
            // runtime objects. Allocation ownership is the release oracle.
            if value.is_cons() {
                assert!(
                    !heap.value_is_marked_for_test(value),
                    "{owner}: released captured cons must not remain marked"
                );
            }
        }
    });
}

fn check_resolved_layout_vars_roots(bounded: bool) {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 4096 ?x))
             (make-local-variable 'line-prefix)
             (make-local-variable 'wrap-prefix)
             (set 'line-prefix (make-string 7 ?L))
             (set 'wrap-prefix (make-string 9 ?W))
             (set 'snapshot-root2-calls 0)
             (set 'fontification-functions
               (list (lambda (_pos)
                       (set 'snapshot-root2-calls (1+ snapshot-root2-calls))
                       (set 'line-prefix (make-string 11 ?l))
                       (set 'wrap-prefix (make-string 13 ?w))
                       (garbage-collect)))))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let buffer_id = buffer.id();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        bounded.then_some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let snapshot = {
        let retained = snapshot.clone();
        drop(snapshot);
        retained
    };
    let line = snapshot
        .layout_buffer_local_value(LayoutVar::LinePrefix)
        .unwrap();
    let wrap = snapshot
        .layout_buffer_local_value(LayoutVar::WrapPrefix)
        .unwrap();
    assert_eq!(line.as_str_owned().as_deref(), Some("LLLLLLL"));
    assert_eq!(wrap.as_str_owned().as_deref(), Some("WWWWWWWWW"));

    // No Lisp global, quoted literal, or callback closure retains either
    // old string. The callback replaces the two live buffer-local sources.
    run_replacing_fontifier(&mut eval, buffer_id);
    assert_snapshot_values_alive(&[line, wrap], "resolved LayoutVars");
    assert_eq!(
        snapshot
            .layout_buffer_local_value(LayoutVar::LinePrefix)
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("LLLLLLL"),
        "the pending view must retain its original line-prefix"
    );
    assert_eq!(
        snapshot
            .layout_buffer_local_value(LayoutVar::WrapPrefix)
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("WWWWWWWWW"),
        "the pending view must retain its original wrap-prefix"
    );
    assert_eq!(
        eval.eval_str("line-prefix")
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("lllllllllll"),
        "the live source must actually have changed"
    );
    assert_eq!(
        eval.eval_str("wrap-prefix")
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("wwwwwwwwwwwww")
    );
    drop(snapshot);
    eval.gc_collect_exact();
    assert_snapshot_values_reclaimed(&[line, wrap], "resolved LayoutVars");
}

fn check_category_symbol_plists_roots(bounded: bool) {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 4096 ?x))
             (setplist 'snapshot-root2-overlay-category
               (list 'before-string (make-string 5 ?O)))
             (setplist 'snapshot-root2-text-category
               (list 'before-string (make-string 6 ?T)))
             (overlay-put (make-overlay 2 6)
               'category 'snapshot-root2-overlay-category)
             (put-text-property 8 10 'category 'snapshot-root2-text-category)
             (set 'snapshot-root2-calls 0)
             (set 'fontification-functions
               (list (lambda (_pos)
                       (set 'snapshot-root2-calls (1+ snapshot-root2-calls))
                       (setplist 'snapshot-root2-overlay-category
                         (list 'before-string (make-string 8 ?o)))
                       (setplist 'snapshot-root2-text-category
                         (list 'before-string (make-string 9 ?t)))
                       (garbage-collect)))))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let buffer_id = buffer.id();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        bounded.then_some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let snapshot = {
        let retained = snapshot.clone();
        drop(snapshot);
        retained
    };
    assert_eq!(snapshot.category_symbol_plists.len(), 2);
    let categories = [
        (Value::symbol("snapshot-root2-overlay-category"), "OOOOO"),
        (Value::symbol("snapshot-root2-text-category"), "TTTTTT"),
    ];
    let property = Value::symbol("before-string");
    let mut old_operands = Vec::new();
    for &(category, expected) in &categories {
        let mut tail = snapshot.category_symbol_plists[&category.as_bare_symbol_id().unwrap()];
        while tail.is_cons() {
            old_operands.push(tail);
            tail = tail.cons_cdr();
        }
        let value = snapshot
            .layout_category_symbol_property(category, property)
            .unwrap();
        assert_eq!(value.as_str_owned().as_deref(), Some(expected));
        old_operands.push(value);
    }
    assert_eq!(
        old_operands.len(),
        6,
        "two cells and one string per category"
    );

    // Root1's overlay-header lease keeps the category reference, not the
    // replaced plist spine held in category_symbol_plists. setplist creates
    // an independent new spine; put on an existing property would mutate it.
    run_replacing_fontifier(&mut eval, buffer_id);
    assert_snapshot_values_alive(&old_operands, "captured category symbol plists");
    for (category, expected) in categories {
        assert_eq!(
            snapshot
                .layout_category_symbol_property(category, property)
                .unwrap()
                .as_str_owned()
                .as_deref(),
            Some(expected),
            "the pending view must read its original category property"
        );
    }
    assert_eq!(
        eval.eval_str("(get 'snapshot-root2-overlay-category 'before-string)")
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("oooooooo"),
        "the overlay category's current plist must actually have changed"
    );
    assert_eq!(
        eval.eval_str("(get 'snapshot-root2-text-category 'before-string)")
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("ttttttttt")
    );
    drop(snapshot);
    eval.gc_collect_exact();
    assert_snapshot_values_reclaimed(&old_operands, "captured category symbol plists");
}

#[test]
fn bounded_snapshot_resolved_layout_vars_survive_replacement_and_fontification_gc() {
    check_resolved_layout_vars_roots(true);
}

#[test]
fn full_snapshot_resolved_layout_vars_survive_replacement_and_fontification_gc() {
    check_resolved_layout_vars_roots(false);
}

#[test]
fn bounded_snapshot_category_symbol_plists_survive_replacement_and_fontification_gc() {
    check_category_symbol_plists_roots(true);
}

#[test]
fn full_snapshot_category_symbol_plists_survive_replacement_and_fontification_gc() {
    check_category_symbol_plists_roots(false);
}
