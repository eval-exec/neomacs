//! Packed bool-vectors (`BoolVectorObj`, P3.2 L0.2) survive a pdump round
//! trip: their bits, their identity where two holders share one, `equal`
//! hash-table keys, and a category table's shared category sets.
use super::*;

fn eval(ctx: &mut Context, src: &str) -> Value {
    let v = ctx.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn printed(ctx: &mut Context, src: &str) -> String {
    crate::emacs_core::print::print_value(&eval(ctx, src))
}

#[test]
fn packed_bool_vectors_round_trip_with_their_identity() {
    crate::test_utils::init_test_tracing();
    let scratch_base = crate::emacs_core::eval::save_scratch_gc_roots();
    let mut ctx = Context::new();
    eval(
        &mut ctx,
        "(progn
           (defvar pbv-a (make-bool-vector 70 nil))
           (aset pbv-a 0 t) (aset pbv-a 64 t) (aset pbv-a 69 t)
           (defvar pbv-shared (list pbv-a pbv-a))
           (defvar pbv-empty (make-bool-vector 0 t))
           (defvar pbv-table (make-hash-table :test 'equal))
           (puthash (bool-vector t nil t) 'small pbv-table)
           (puthash (make-bool-vector 200 t) 'large pbv-table)
           (define-category ?Q \"pbv test category\")
           (modify-category-entry ?q ?Q)
           (defvar pbv-set (char-category-set ?q)))",
    );
    assert!(eval(&mut ctx, "pbv-a").is_bool_vector_obj());

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bool-vectors.pdump");
    dump_to_file(&ctx, &dump_path).expect("dump should succeed");
    crate::emacs_core::eval::restore_scratch_gc_roots(scratch_base);
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");

    assert!(eval(&mut loaded, "pbv-a").is_bool_vector_obj());
    assert_eq!(
        printed(
            &mut loaded,
            "(list (bool-vector-p pbv-a) (length pbv-a) (aref pbv-a 0) (aref pbv-a 1) \
                   (aref pbv-a 64) (aref pbv-a 69) (bool-vector-count-population pbv-a) \
                   (eq (car pbv-shared) (car (cdr pbv-shared))) (eq (car pbv-shared) pbv-a) \
                   pbv-empty (length pbv-empty))"
        ),
        "(t 70 t nil t t 3 t t #&0\"\" 0)"
    );
    // Equal-table keys hash the same after the load.
    assert_eq!(
        printed(
            &mut loaded,
            "(list (gethash (bool-vector t nil t) pbv-table) \
                   (gethash (make-bool-vector 200 t) pbv-table) \
                   (gethash (make-bool-vector 200 nil) pbv-table))"
        ),
        "(small large nil)"
    );
    // The category set came back as a packed bool-vector the table still
    // answers with.
    assert_eq!(
        printed(
            &mut loaded,
            "(list (bool-vector-p pbv-set) (length pbv-set) (aref pbv-set ?Q) \
                   (eq pbv-set (char-category-set ?q)) \
                   (category-set-mnemonics (char-category-set ?q)))"
        ),
        "(t 128 t t \"Q\")"
    );
    // Mutation after the load is visible through every holder.
    eval(&mut loaded, "(aset pbv-a 1 t)");
    assert_eq!(printed(&mut loaded, "(aref (car (cdr pbv-shared)) 1)"), "t");
    loaded.gc_collect_exact();
    assert_eq!(
        printed(&mut loaded, "(bool-vector-count-population pbv-a)"),
        "4"
    );
}
