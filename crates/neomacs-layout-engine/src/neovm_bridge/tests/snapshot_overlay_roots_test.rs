//! Copied overlay headers must survive actual redisplay fontifier callbacks.
use super::*;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::xdisp::{
    ProbeRow, RedisplayFontificationRequest, ensure_fontified_for_redisplay_for_window_request,
};

/// Count how many root visits each of `targets` (numeric Value bits) gets in
/// the evaluator's `owned_native` (lease) and `buffers` (buffer/memo) groups.
/// The counts come from the real root registry, so a buffer-owned memo copy
/// can never stand in for a live snapshot lease.
fn root_group_counts(eval: &Context, targets: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let heap_id = neovm_core::tagged::gc::with_tagged_heap(|heap| heap.tagged_heap_identity());
    let mut owned = vec![0usize; targets.len()];
    let mut buffered = vec![0usize; targets.len()];
    let current = std::cell::Cell::new("");
    eval.trace_roots(
        heap_id,
        |roots| {
            neovm_core::tagged::gc::with_tagged_heap(|heap| {
                neovm_value::tagged::transport::root_table::collect_shared_root_gc_roots(
                    heap, roots,
                );
            });
        },
        &mut |group| current.set(group),
        &mut |value| {
            let Some(index) = targets.iter().position(|target| *target == value.bits()) else {
                return;
            };
            match current.get() {
                "owned_native" => owned[index] += 1,
                "buffers" => buffered[index] += 1,
                _ => {}
            }
        },
    );
    (owned, buffered)
}

fn check_snapshot_fontification_roots(bounded: bool) {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 4096 ?x))
             (setq snapshot-root-calls 0 snapshot-root-overlays nil)
             (let ((i 0))
               (while (< i 2)
                 (let ((ov (make-overlay (+ 2 (* i 8)) (+ 6 (* i 8)))))
                   (overlay-put ov 'face 'bold)
                   (overlay-put ov 'before-string "m")
                   (setq snapshot-root-overlays (cons ov snapshot-root-overlays)))
                 (setq i (1+ i))))
             (setq fontification-functions
               (list (lambda (_pos)
                       (setq snapshot-root-calls (1+ snapshot-root-calls))
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
    let pre_bits: Vec<usize> = snapshot
        .layout_overlays()
        .overlays_in_emacs_byte_range(EmacsByteRange::from_usize(0, 64))
        .iter()
        .map(|copy| copy.bits())
        .collect();
    let (leased_pre, _) = root_group_counts(&eval, &pre_bits);
    assert!(
        leased_pre.iter().all(|count| *count > 0),
        "the pending snapshot must lease every copied overlay"
    );
    let snapshot = {
        let retained = snapshot.clone();
        drop(snapshot);
        retained
    };
    let (leased_clone, _) = root_group_counts(&eval, &pre_bits);
    assert_eq!(
        leased_clone, leased_pre,
        "dropping a nonfinal clone must not retire the snapshot lease"
    );
    let copied = snapshot
        .layout_overlays()
        .overlays_in_emacs_byte_range(EmacsByteRange::from_usize(0, 64));
    assert_eq!(copied.len(), 2, "two independently copied overlay plists");
    let addresses: Vec<_> = copied
        .iter()
        .map(|copy| copy.as_veclike_ptr().unwrap() as usize)
        .collect();
    let mut plist_cells = Vec::new();
    for copy in &copied {
        let mut cell = copy.as_overlay_data().unwrap().plist;
        while cell.is_cons() {
            plist_cells.push(cell);
            cell = cell.cons_cdr();
        }
    }
    assert_eq!(
        plist_cells.len(),
        8,
        "four independent copied cells per overlay"
    );
    if !bounded {
        // Retiring the live buffer's full-snapshot cache must not unroot an
        // already-held view. The pending copy still reads its old property.
        eval.eval_str("(overlay-put (car snapshot-root-overlays) 'face 'italic)")
            .unwrap();
    }
    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();

    // This is the same callback-bearing helper used by the layout engine's
    // visible fontification pass, while its LayoutBufferSnapshot is alive.
    ensure_fontified_for_redisplay_for_window_request(
        &mut eval,
        buffer_id,
        1,
        2,
        RedisplayFontificationRequest::VisibleWalk,
        ProbeRow::None,
        1,
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("snapshot-root-calls").unwrap().as_int(),
        Some(1),
        "the actual fontification function must have forced exact GC"
    );
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "the callback's garbage-collect must complete an actual collection"
    );
    // Check numeric allocation-catalog membership BEFORE any potentially
    // stale copied Value is dereferenced. A red run must fail without a UAF.
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        for &address in &addresses {
            assert!(
                heap.non_cons_object_addrs_contains_for_test(address),
                "snapshot copied overlay lost across fontification GC; bounded={bounded}"
            );
        }
        for cell in &plist_cells {
            assert!(
                heap.owns_heap_value_for_test(*cell),
                "snapshot copied plist cell lost across callback GC"
            );
            assert!(heap.value_is_marked_for_test(*cell));
        }
    });
    for copy in &copied {
        assert_eq!(
            snapshot
                .layout_overlays()
                .overlay_get_named(*copy, Value::symbol("face")),
            Some(Value::symbol("bold")),
            "pending view must retain its copied plist after callback GC"
        );
    }
    // While the first view is held, invalidate one of its copies in the
    // memo (a live overlay write) and take a second bounded view. The old
    // copy must survive under the first view's lease, leave the buffer group,
    // and be physically reclaimed after that view drops; the new copy is
    // memo-owned until the overlays are cleared.
    let mut second_addresses: Vec<usize> = Vec::new();
    let mut second_fresh_bits: Vec<usize> = Vec::new();
    if bounded {
        eval.eval_str("(overlay-put (car snapshot-root-overlays) 'face 'italic)")
            .unwrap();
        let buffer = eval.buffer_manager().current_buffer().unwrap();
        let second = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
            buffer,
            eval.variables(),
            Some((0, 64)),
            Some((0, 64)),
            crate::display_property::DisplayPropertyTarget::Terminal,
            eval.symbol_mode(),
            &eval.owned_root_retainer(),
        );
        let second_copies: Vec<_> = second
            .layout_overlays()
            .overlays_in_emacs_byte_range(EmacsByteRange::from_usize(0, 64));
        let second_bits: Vec<usize> = second_copies.iter().map(|copy| copy.bits()).collect();
        second_addresses = second_copies
            .iter()
            .map(|copy| copy.as_veclike_ptr().unwrap() as usize)
            .collect();
        second_fresh_bits = second_bits
            .iter()
            .copied()
            .filter(|bits| !pre_bits.contains(bits))
            .collect();
        assert_eq!(
            second_fresh_bits.len(),
            1,
            "one invalidated copy must be fresh"
        );
        let (second_leased, second_buffered) = root_group_counts(&eval, &second_bits);
        assert!(second_leased.iter().all(|count| *count > 0));
        assert!(second_buffered.iter().all(|count| *count > 0));
        let (first_leased_now, first_buffered_now) = root_group_counts(&eval, &pre_bits);
        assert!(
            first_leased_now.iter().all(|count| *count > 0),
            "the held view must keep leasing its captured copies"
        );
        let evicted = first_buffered_now
            .iter()
            .filter(|count| **count == 0)
            .count();
        assert_eq!(
            evicted, 1,
            "the write must evict exactly the changed memo copy"
        );
        drop(second);
        eval.gc_collect_exact();
        let (fresh_leased_after, fresh_buffered_after) =
            root_group_counts(&eval, &second_fresh_bits);
        assert!(
            fresh_leased_after.iter().all(|count| *count == 0),
            "dropping the second view must retire the fresh copy's lease"
        );
        assert!(
            fresh_buffered_after.iter().all(|count| *count > 0),
            "the bounded memo must own the fresh copy"
        );
        // The memo no longer owns the changed old copy. Its header and plist
        // must still be live under the first view's lease, checked BEFORE any
        // dereference, and its old value must still read back.
        neovm_core::tagged::gc::with_tagged_heap(|heap| {
            for address in &addresses {
                assert!(
                    heap.non_cons_object_addrs_contains_for_test(*address),
                    "a held copy's header was lost after the memo eviction GC"
                );
            }
            for cell in &plist_cells {
                assert!(
                    heap.owns_heap_value_for_test(*cell),
                    "a held copy's plist cell was lost after the memo eviction GC"
                );
                assert!(heap.value_is_marked_for_test(*cell));
            }
        });
        for copy in &copied {
            assert_eq!(
                snapshot
                    .layout_overlays()
                    .overlay_get_named(*copy, Value::symbol("face")),
                Some(Value::symbol("bold")),
                "the held view must keep reading its old value after memo eviction"
            );
        }
    }

    // Why each copied overlay is live is as important as whether it is: the
    // snapshot's native lease and the buffer's bounded range-view memo are
    // different owners. Re-read the real lease counts before the final drop.
    let (leased_before, _) = root_group_counts(&eval, &pre_bits);
    assert!(
        leased_before.iter().all(|count| *count > 0),
        "the held snapshot must still lease every copied overlay"
    );

    drop(snapshot);
    eval.gc_collect_exact();
    let (leased_after, buffered_after) = root_group_counts(&eval, &pre_bits);
    assert!(
        leased_after.iter().all(|count| *count == 0),
        "dropping the final snapshot must retire every copied overlay lease"
    );
    if bounded {
        // R3 retains range-view copies in the buffer's bounded memo so the
        // next redisplay can reuse them. Every surviving copy must therefore
        // be owned by the buffer group; the snapshot lease above is gone.
        let memo_owned = buffered_after.iter().filter(|count| **count > 0).count();
        assert_eq!(
            memo_owned,
            buffered_after.len() - 1,
            "the bounded memo must own exactly the unchanged copies (the invalidated one was evicted)"
        );
        let evicted_index = buffered_after
            .iter()
            .position(|count| *count == 0)
            .expect("the invalidated copy must have left the memo");
        neovm_core::tagged::gc::with_tagged_heap(|heap| {
            assert!(
                !heap.non_cons_object_addrs_contains_for_test(addresses[evicted_index]),
                "the memo-evicted old copy must be reclaimed as soon as its lease drops"
            );
        });
        eval.eval_str(
            "(mapc #'delete-overlay (append (car (overlay-lists)) (cdr (overlay-lists))))",
        )
        .unwrap();
        eval.gc_collect_exact();
    } else {
        assert!(
            buffered_after.iter().all(|count| *count == 0),
            "a full snapshot's copies must not be retained by the buffer"
        );
        neovm_core::tagged::gc::with_tagged_heap(|heap| {
            for address in &addresses {
                assert!(
                    !heap.non_cons_object_addrs_contains_for_test(*address),
                    "a full snapshot's copy must be reclaimed as soon as its lease drops"
                );
            }
        });
    }
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        for address in addresses.iter().chain(second_addresses.iter()) {
            assert!(
                !heap.non_cons_object_addrs_contains_for_test(*address),
                "dropping the final snapshot (and any bounded memo owner) must release its copied overlay roots"
            );
        }
    });
}

#[test]
fn bounded_snapshot_overlay_copies_survive_fontification_gc() {
    check_snapshot_fontification_roots(true);
}

#[test]
fn retired_full_snapshot_overlay_copies_survive_fontification_gc() {
    check_snapshot_fontification_roots(false);
}

#[test]
fn held_snapshot_text_property_operands_survive_replacing_fontifier_gc() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (setq buffer-undo-list t snapshot-root2-calls 0)
             (insert (make-string 4096 ?x))
             (put-text-property 1 2 'face (list :foreground (make-string 17 ?q)))
             (setq fontification-functions
               (list (lambda (_pos)
                       (setq snapshot-root2-calls (1+ snapshot-root2-calls))
                       (set-text-properties 1 2 (list 'face 'italic))
                       (garbage-collect)))))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let buffer_id = buffer.id();
    let old_plist = buffer
        .interval_plist_at_emacs_byte_pos(EmacsBytePos::new(0))
        .unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        Some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let old_face = snapshot
        .layout_text_prop_at_emacs_byte_pos(EmacsBytePos::new(0), Value::symbol("face"))
        .unwrap();
    let old_string = old_face.cons_cdr().cons_car();
    assert_eq!(old_string.as_str_owned(), Some("q".repeat(17)));
    // Save every cell the later view read traverses while it is still live.
    // Plain Rust Values are deliberately not registered as native GC roots.
    let mut old_cells = Vec::new();
    for mut cell in [old_plist, old_face] {
        while cell.is_cons() {
            old_cells.push(cell);
            cell = cell.cons_cdr();
        }
    }
    assert_eq!(old_cells.len(), 4, "outer property plist and face plist");
    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();

    ensure_fontified_for_redisplay_for_window_request(
        &mut eval,
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
        "the actual redisplay fontifier must have run"
    );
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "the fontifier must complete a collection"
    );
    assert_eq!(
        eval.eval_str("(get-text-property 1 'face)").unwrap(),
        Value::symbol("italic"),
        "the live buffer must have replaced the only old Lisp reference"
    );
    // Allocation membership uses tag/address metadata and cons free lists.
    // Do NOT read the held view or any old payload before all guards pass.
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        assert!(
            heap.owns_heap_value_for_test(old_string),
            "Rc-held text-property string lost across replacing fontifier GC"
        );
        for &cell in &old_cells {
            assert!(
                heap.owns_heap_value_for_test(cell),
                "Rc-held text-property plist cell lost across replacing fontifier GC"
            );
        }
    });
    let retained_face = snapshot
        .layout_text_prop_at_emacs_byte_pos(EmacsBytePos::new(0), Value::symbol("face"))
        .unwrap();
    assert_eq!(
        retained_face, old_face,
        "the pending view keeps its old table"
    );
    assert_eq!(retained_face.cons_car(), Value::symbol(":foreground"));
    assert_eq!(
        retained_face.cons_cdr().cons_car().as_str_owned(),
        Some("q".repeat(17))
    );
    drop(snapshot);
    eval.gc_collect_exact();
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        assert!(
            !heap.owns_heap_value_for_test(old_string),
            "retiring the final old table must release its operands"
        );
    });
}

#[derive(Clone, Copy, Debug)]
enum SnapshotCompositionRoot {
    RuleTable,
    CategoryTable,
    SyntaxTable,
}

fn check_snapshot_composition_rules_survive_fontification_gc(root: SnapshotCompositionRoot) {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 4096 ?x))
             (setq auto-composition-mode t
                   auto-composition-function 'auto-compose-chars
                   composition-function-table (make-char-table nil)
                   snapshot-composition-calls 0)
             (set-category-table (make-category-table))
             (define-category ?Q "snapshot private category")
             (modify-category-entry ?x ?Q)
             (aset composition-function-table ?x
                   (list (vector "\\cQy" 0 'font-shape-gstring))))"#,
    )
    .unwrap();
    if matches!(root, SnapshotCompositionRoot::SyntaxTable) {
        eval.eval_str("(set-syntax-table (copy-syntax-table))")
            .unwrap();
    }
    // These are the original inputs captured by AutomaticCompositionRules.
    // Rust Value locals deliberately do not register roots. Do not bind either
    // original table to a second Lisp variable, which would hide the defect.
    let table = eval.eval_str("composition-function-table").unwrap();
    let category = eval.eval_str("(category-table)").unwrap();
    let syntax = eval.eval_str("(syntax-table)").unwrap();
    let replacement = match root {
        SnapshotCompositionRoot::RuleTable => {
            "(setq composition-function-table (make-char-table nil))"
        }
        SnapshotCompositionRoot::CategoryTable => "(set-category-table (make-category-table))",
        SnapshotCompositionRoot::SyntaxTable => "(set-syntax-table (copy-syntax-table))",
    };
    eval.eval_str(&format!(
        "(setq fontification-functions
           (list (lambda (_pos)
                   (setq snapshot-composition-calls (1+ snapshot-composition-calls))
                   {replacement}
                   (garbage-collect))))"
    ))
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let buffer_id = buffer.id();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        Some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let snapshot = {
        let retained = snapshot.clone();
        drop(snapshot);
        retained
    };
    // The fields of AutomaticCompositionRules are private in neovm-text.
    // Some + this category-sensitive match establish that the actual capture
    // uses both original Values, rather than a nil/default composition path.
    let rules = snapshot
        .string_composition_rules
        .expect("active captured rules");
    let before_spans = rules.spans("xy");
    assert_eq!(before_spans.len(), 1, "custom table/category must match");
    assert_eq!((before_spans[0].start(), before_spans[0].end()), (0, 2));
    let target = match root {
        SnapshotCompositionRoot::RuleTable => table,
        SnapshotCompositionRoot::CategoryTable => category,
        SnapshotCompositionRoot::SyntaxTable => syntax,
    };
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        assert!(heap.owns_heap_value_for_test(target));
    });
    let before_gc = eval
        .variables()
        .value_by_name("gcs-done")
        .unwrap()
        .as_int()
        .unwrap();
    ensure_fontified_for_redisplay_for_window_request(
        &mut eval,
        buffer_id,
        1,
        2,
        RedisplayFontificationRequest::VisibleWalk,
        ProbeRow::None,
        1,
    )
    .unwrap();
    assert_eq!(
        eval.variables()
            .value_by_name("snapshot-composition-calls")
            .unwrap()
            .as_int(),
        Some(1),
        "the real fontifier must run exactly once"
    );
    assert!(
        eval.variables()
            .value_by_name("gcs-done")
            .unwrap()
            .as_int()
            .unwrap()
            > before_gc
    );
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        // Numeric ownership metadata only, before any captured rule can read
        // either potentially reclaimed table. Each target has its own test.
        assert!(
            heap.owns_heap_value_for_test(target),
            "snapshot composition root lost across callback GC: {root:?}"
        );
        assert!(heap.owns_heap_value_for_test(table));
        assert!(heap.owns_heap_value_for_test(category));
        assert!(heap.owns_heap_value_for_test(syntax));
    });
    let after_spans = snapshot
        .string_composition_rules
        .expect("pending view retains its rules")
        .spans("xy");
    assert_eq!(after_spans.len(), 1);
    assert_eq!((after_spans[0].start(), after_spans[0].end()), (0, 2));
    assert_ne!(
        match root {
            SnapshotCompositionRoot::RuleTable => eval.eval_str("composition-function-table"),
            SnapshotCompositionRoot::CategoryTable => eval.eval_str("(category-table)"),
            SnapshotCompositionRoot::SyntaxTable => eval.eval_str("(syntax-table)"),
        }
        .unwrap()
        .bits(),
        target.bits(),
        "callback must actually replace the source captured by the pending view"
    );
    // AutomaticCompositionRules is a Copy view containing raw Values; the
    // local `rules` does not own a root. Only the snapshot lease retains it.
    drop(snapshot);
    eval.gc_collect_exact();
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        assert!(
            !heap.owns_heap_value_for_test(target),
            "final snapshot drop must release its replaced composition input: {root:?}"
        );
        // An unowned non-cons address is treated as permanent by mark
        // queries; the allocation catalog above proves reclamation.
    });
}

#[test]
fn snapshot_composition_rule_table_survives_fontification_gc() {
    check_snapshot_composition_rules_survive_fontification_gc(SnapshotCompositionRoot::RuleTable);
}

#[test]
fn snapshot_composition_category_table_survives_fontification_gc() {
    check_snapshot_composition_rules_survive_fontification_gc(
        SnapshotCompositionRoot::CategoryTable,
    );
}

#[test]
fn snapshot_composition_syntax_table_survives_fontification_gc() {
    check_snapshot_composition_rules_survive_fontification_gc(SnapshotCompositionRoot::SyntaxTable);
}

#[derive(Clone, Copy, Debug)]
enum InitialDisplayWhenRoot {
    Form,
    Object,
}

fn check_initial_display_when_answers_survive_fontification_gc(root: InitialDisplayWhenRoot) {
    use crate::display_when::{
        DisplayWhenLookup, DisplayWhenVerdict, evaluate_window_display_when_forms,
    };

    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    // Match engine.rs's logical-frame scope. The scope alone enumerates no
    // Values: this fixture must not manually push the initial answer keys.
    let _display_when_gc_roots = crate::display_status_line::ScratchGcRootScope::new();
    eval.eval_str(
        r#"(progn
             (setq buffer-undo-list t)
             (insert (make-string 4096 ?x))
             (setq snapshot-when-calls 0
                   snapshot-when-source (make-string 2 ?p))
             (let ((form
                    (list 'eq (list 'identity nil) t)))
               (put-text-property 0 2 'display
                                  (cons 'when (cons form "replacement"))
                                  snapshot-when-source)
               (put-text-property 1 3 'line-prefix snapshot-when-source))
             (setq fontification-functions
               (list (lambda (_pos)
                       (setq snapshot-when-calls (1+ snapshot-when-calls))
                       (put-text-property 1 3 'line-prefix nil)
                       (put-text-property 0 2 'display nil snapshot-when-source)
                       (setq snapshot-when-source nil)
                       (garbage-collect)))))"#,
    )
    .unwrap();
    let source = eval.eval_str("snapshot-when-source").unwrap();
    let form = eval
        .eval_str("(car (cdr (get-text-property 0 'display snapshot-when-source)))")
        .unwrap();
    assert!(source.is_string());
    assert!(
        form.is_cons(),
        "fresh nonliteral FORM, never nil/t fast path"
    );
    // Check the actual heap form before either evaluation or collection, so
    // a fixture/normalization mistake cannot masquerade as a GC failure.
    assert!(form.cons_car().is_symbol_named("eq", eval.symbol_mode()));
    let form_tail = form.cons_cdr();
    let form_last = form_tail.cons_cdr();
    let nested = form_tail.cons_car();
    let nested_tail = nested.cons_cdr();
    let form_cells = [form, form_tail, form_last, nested, nested_tail];
    assert!(
        nested
            .cons_car()
            .is_symbol_named("identity", eval.symbol_mode())
    );
    assert!(nested_tail.cons_car().is_nil());
    assert!(nested_tail.cons_cdr().is_nil());
    assert!(form_last.cons_car().is_t());
    assert!(form_last.cons_cdr().is_nil());
    assert_eq!(source.as_utf8_str(), Some("pp"));
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let lookup = DisplayWhenLookup::String {
        string: source,
        index: 0,
        anchor: Some(1),
    };
    let conditions = evaluate_window_display_when_forms(
        &mut eval,
        buffer_id,
        None,
        CharPos0::new(0),
        CharPos0::new(64),
        crate::display_property::DisplayPropertyTarget::Terminal,
    )
    .with_visit_recording();
    assert_eq!(conditions.verdict(form, lookup), DisplayWhenVerdict::Fails);
    assert!(
        !conditions.has_recorded_visits(),
        "this covers initial answers only"
    );
    assert_eq!(
        eval.eval_str("(get-text-property 1 'line-prefix)").unwrap(),
        source,
        "the prepass must leave the source unchanged for normal admission"
    );
    assert!(
        form.cons_car().is_symbol_named("eq", eval.symbol_mode()),
        "the production prepass must preserve the FORM before callback GC"
    );
    assert!(
        nested
            .cons_car()
            .is_symbol_named("identity", eval.symbol_mode())
    );
    // Production also evaluates conditions before constructing this snapshot.
    // The callback detaches FORM by clearing this shared string's display
    // property. A later snapshot text-table root may protect the string object
    // transitively; it cannot protect FORM once that property is cleared.
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        Some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    )
    .with_display_when(conditions);
    let snapshot = {
        let retained = snapshot.clone();
        drop(snapshot);
        retained
    };
    let before_gc = eval
        .variables()
        .value_by_name("gcs-done")
        .unwrap()
        .as_int()
        .unwrap();
    ensure_fontified_for_redisplay_for_window_request(
        &mut eval,
        buffer_id,
        1,
        2,
        RedisplayFontificationRequest::VisibleWalk,
        ProbeRow::None,
        1,
    )
    .unwrap();
    assert_eq!(
        eval.variables()
            .value_by_name("snapshot-when-calls")
            .unwrap()
            .as_int(),
        Some(1)
    );
    assert!(
        eval.variables()
            .value_by_name("gcs-done")
            .unwrap()
            .as_int()
            .unwrap()
            > before_gc
    );
    assert!(
        eval.variables()
            .value_by_name("snapshot-when-source")
            .unwrap()
            .is_nil()
    );
    let check_captured_root = |phase: &str| {
        neovm_core::tagged::gc::with_tagged_heap(|heap| {
            // Only check the class this row names. In particular, the object
            // row must not fail because FORM was collected. Neither guard
            // reads an old payload; a freed/reused cons needs both checks.
            let values: &[Value] = match root {
                InitialDisplayWhenRoot::Form => &form_cells,
                InitialDisplayWhenRoot::Object => std::slice::from_ref(&source),
            };
            for &value in values {
                assert!(
                    heap.owns_heap_value_for_test(value),
                    "initial display-WHEN {root:?} reclaimed: {phase}"
                );
                assert!(
                    heap.value_is_marked_for_test(value),
                    "initial display-WHEN {root:?} unmarked: {phase}"
                );
            }
        });
    };
    check_captured_root("callback GC");
    // The hook has now returned. Collect again without evaluating another
    // Lisp form: callback/eval temporaries cannot supply an incidental root
    // for the pending snapshot. This is a lifetime boundary, not GC stress.
    eval.gc_collect_exact();
    check_captured_root("post-callback exact GC");
    match root {
        InitialDisplayWhenRoot::Form => {
            assert!(form.cons_car().is_symbol_named("eq", eval.symbol_mode()));
            assert_eq!(form.cons_cdr(), form_tail);
            assert_eq!(form_tail.cons_car(), nested);
            assert_eq!(form_tail.cons_cdr(), form_last);
            assert!(
                nested
                    .cons_car()
                    .is_symbol_named("identity", eval.symbol_mode())
            );
            assert_eq!(nested.cons_cdr(), nested_tail);
            assert!(nested_tail.cons_car().is_nil());
            assert!(nested_tail.cons_cdr().is_nil());
            assert!(form_last.cons_car().is_t());
            assert!(form_last.cons_cdr().is_nil());
        }
        InitialDisplayWhenRoot::Object => assert_eq!(source.as_utf8_str(), Some("pp")),
    }
    assert_eq!(
        snapshot.display_when_conditions().verdict(form, lookup),
        DisplayWhenVerdict::Fails,
        "the pending view must preserve its initially evaluated false answer"
    );
    assert!(!snapshot.display_when_conditions().has_recorded_visits());
    drop(snapshot);
    eval.gc_collect_exact();
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        // The final pending-view owner is gone. Do not query Lisp or inspect
        // a payload after collecting: these raw Values are identities only.
        let values: &[Value] = match root {
            InitialDisplayWhenRoot::Form => &form_cells,
            InitialDisplayWhenRoot::Object => std::slice::from_ref(&source),
        };
        for &value in values {
            assert!(
                !heap.owns_heap_value_for_test(value),
                "final snapshot drop must release initial display-WHEN {root:?}"
            );
            // Only query ownership after reclamation: non-cons mark queries
            // default to permanent for addresses outside the heap catalog.
            if value.is_cons() {
                assert!(
                    !heap.value_is_marked_for_test(value),
                    "released initial display-WHEN {root:?} must not remain marked"
                );
            }
        }
    });
}

#[test]
fn snapshot_initial_display_when_form_survives_fontification_gc() {
    check_initial_display_when_answers_survive_fontification_gc(InitialDisplayWhenRoot::Form);
}

#[test]
fn snapshot_initial_display_when_object_survives_fontification_gc() {
    check_initial_display_when_answers_survive_fontification_gc(InitialDisplayWhenRoot::Object);
}

#[test]
fn layout_stats_capture_counts_retained_overlay_roots() {
    // SAFETY: nextest runs this test in its own process. The stats OnceLock
    // must see the path before capture reads it.
    let dir = std::env::temp_dir().join(format!(
        "neomacs-overlay-layout-stats-roots-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("overlay layout stats tempdir");
    unsafe {
        std::env::set_var("NEOMACS_LAYOUT_STATS_FILE", dir.join("stats"));
    }
    neovm_core::buffer::overlay::reset_overlay_layout_stats();
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 64 ?x))
             (make-overlay 1 8)
             (make-overlay 10 20))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let before = neovm_core::buffer::overlay::overlay_layout_stats();
    let _snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        None,
        None,
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let after = neovm_core::buffer::overlay::overlay_layout_stats();
    assert_eq!(
        after.roots - before.roots,
        2,
        "capture leases both copied overlays: before={before:?} after={after:?}"
    );
    assert_eq!(after.rebuilds - before.rebuilds, 1, "{before:?} {after:?}");
    assert_eq!(after.copies - before.copies, 2, "{before:?} {after:?}");
    assert_eq!(
        after.view_entries - before.view_entries,
        2,
        "{before:?} {after:?}"
    );
    assert_eq!(after.live_records, 0, "{after:?}");
}

#[test]
fn layout_stats_off_leaves_all_five_counters_at_zero() {
    // SAFETY: nextest runs this test in its own process. Clear the knob
    // before the first read so the OnceLock records stats-off.
    unsafe {
        std::env::remove_var("NEOMACS_LAYOUT_STATS_FILE");
    }
    neovm_core::buffer::overlay::reset_overlay_layout_stats();
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 64 ?x))
             (make-overlay 1 8)
             (make-overlay 10 20))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let _snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        None,
        None,
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let stats = neovm_core::buffer::overlay::overlay_layout_stats();
    assert_eq!(
        stats,
        neovm_core::buffer::overlay::OverlayLayoutStats::default(),
        "a layout-engine test build must not count with the stats file unset: {stats:?}"
    );
}
