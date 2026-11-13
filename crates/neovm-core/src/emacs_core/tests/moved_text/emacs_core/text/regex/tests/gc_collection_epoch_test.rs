use super::*;
use std::time::Duration;

use neovm_edit::buffer::{
    Buffer, BufferId, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use neovm_text::emacs_core::casefiddle::apply_replace_match_case;
use neovm_text::emacs_core::casefiddle::{
    apply_replace_match_case_lisp, apply_replace_match_case_lisp_with,
};
use neovm_text::emacs_core::syntax::SyntaxClass;
use neovm_value::emacs_char::char_to_byte_pos;
use neovm_value::tagged::SymbolMode;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::emacs_core::casetab_entries::builtin_set_case_table;
use crate::emacs_core::eval::Context;
use neovm_text::emacs_core::regex_emacs;
use neovm_text::emacs_core::regex_emacs::BufferSyntaxLookup;
use neovm_text::emacs_core::regex_emacs::CaseTranslation;
use neovm_text::emacs_core::regex_emacs::CompiledPattern;
use neovm_value::heap_types::LispString;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use neovm_text::emacs_core::regex::*;

use neovm_text::emacs_core::regex_emacs::DefaultSyntaxLookup;
use neovm_value::value::Value;

#[test]
fn gc_collection_epoch_regex_activation_discards_swept_search_syntax() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::SearchSyntax,
        SynchronizeAt::Activation,
    );
}

#[test]
fn gc_collection_epoch_regex_activation_discards_swept_lisp_syntax() {
    assert_uncovered_cache_discards_swept_roots(CacheOwner::LispSyntax, SynchronizeAt::Activation);
}

#[test]
fn gc_collection_epoch_regex_activation_discards_swept_compiled_translation() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::LispTranslation,
        SynchronizeAt::Activation,
    );
}

#[test]
fn gc_collection_epoch_regex_activation_discards_swept_literal_translation() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::LiteralTranslation,
        SynchronizeAt::Activation,
    );
}

#[test]
fn gc_collection_epoch_regex_enumeration_discards_swept_search_syntax() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::SearchSyntax,
        SynchronizeAt::RootEnumeration,
    );
}

#[test]
fn gc_collection_epoch_regex_enumeration_discards_swept_lisp_syntax() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::LispSyntax,
        SynchronizeAt::RootEnumeration,
    );
}

#[test]
fn gc_collection_epoch_regex_enumeration_discards_swept_compiled_translation() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::LispTranslation,
        SynchronizeAt::RootEnumeration,
    );
}

#[test]
fn gc_collection_epoch_regex_enumeration_discards_swept_literal_translation() {
    assert_uncovered_cache_discards_swept_roots(
        CacheOwner::LiteralTranslation,
        SynchronizeAt::RootEnumeration,
    );
}

#[test]
fn gc_collection_epoch_regex_same_thread_exact_keeps_warm_entries() {
    let mut ctx = crate::test_utils::new_context();
    ctx.gc_collect_exact();
    ctx.setup_thread_locals();
    let warm = WarmRegexCaches::new(&mut ctx);
    let completed = ctx.tagged_heap().gc_collections();
    for _ in 0..3 {
        ctx.gc_collect_exact();
        ctx.setup_thread_locals();
        warm.assert_present_and_hit(&ctx);
    }
    assert_eq!(ctx.tagged_heap().gc_collections(), completed + 3);
}

#[test]
fn gc_collection_epoch_regex_same_thread_concurrent_keeps_warm_entries() {
    let mut ctx = non_generational_context();
    ctx.gc_collect_exact();
    ctx.setup_thread_locals();
    let warm = WarmRegexCaches::new(&mut ctx);
    let completed = ctx.tagged_heap().gc_collections();
    for _ in 0..3 {
        start_public_concurrent_cycle(&mut ctx);
        ctx.setup_thread_locals();
        warm.assert_present_and_hit(&ctx);
        drive_public_cycle_until_sweeping(&mut ctx);
        ctx.setup_thread_locals();
        warm.assert_present_and_hit(&ctx);
        for _ in 0..20_000 {
            if !ctx.tagged_heap().sweep_in_progress() {
                break;
            }
            ctx.gc_safe_point();
        }
        assert!(!ctx.tagged_heap().sweep_in_progress());
        ctx.setup_thread_locals();
        warm.assert_present_and_hit(&ctx);
    }
    assert_eq!(ctx.tagged_heap().gc_collections(), completed + 3);
}

#[test]
fn gc_collection_epoch_regex_exact_after_older_sweep_keeps_warm_entries() {
    let mut ctx = non_generational_context();
    ctx.gc_collect_exact();
    ctx.setup_thread_locals();
    let warm = WarmRegexCaches::new(&mut ctx);
    let completed = ctx.tagged_heap().gc_collections();
    start_public_concurrent_cycle(&mut ctx);
    drive_public_cycle_until_sweeping(&mut ctx);
    // Exact GC drains the older sweep before enumerating a new collection's
    // roots. The root seam must use the newly completed count for that seed.
    ctx.gc_collect_exact();
    assert_eq!(ctx.tagged_heap().gc_collections(), completed + 2);
    ctx.setup_thread_locals();
    warm.assert_present_and_hit(&ctx);
}

#[test]
fn gc_collection_epoch_regex_foreign_mark_activation_clears_uncovered_entries() {
    assert_foreign_in_progress_activation_clears(ForeignCollectionPhase::Marking);
}

#[test]
fn gc_collection_epoch_regex_foreign_sweep_activation_clears_uncovered_entries() {
    assert_foreign_in_progress_activation_clears(ForeignCollectionPhase::Sweeping);
}

#[derive(Clone, Copy)]
enum CacheOwner {
    SearchSyntax,
    LispSyntax,
    LispTranslation,
    LiteralTranslation,
}

#[derive(Clone, Copy)]
enum SynchronizeAt {
    Activation,
    RootEnumeration,
}

fn non_generational_context() -> Context {
    // Constructor-read selection, isolated per test by nextest. These tests
    // specifically require concurrent marking rather than a deferred minor.
    // SAFETY: nextest isolates this test process; configuration precedes
    // Context construction and every runtime worker or environment reader.
    unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "0") };
    let mut ctx = crate::test_utils::new_context();
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut ctx, false);
    assert!(!ctx.tagged_heap().tagged_heap_generational_enabled());
    assert!(!ctx.gc_stress_for_test());
    ctx
}

fn assert_uncovered_cache_discards_swept_roots(owner: CacheOwner, synchronize_at: SynchronizeAt) {
    // Keep the owner-local cache outside TLS during collection, then restore
    // its stale coverage metadata without moving the Context across threads.
    let mut ctx = crate::test_utils::new_context();
    // Build the cache fixture without safe-point collections of Rust locals.
    neovm_eval::test_support::authority::enter_gc_inhibit_depth_for_test(&mut ctx);
    let identity = ctx.tagged_heap().tagged_heap_identity();
    let (table, payload) = populate_cache(&mut ctx, owner);
    neovm_eval::test_support::authority::leave_gc_inhibit_depth_for_test(&mut ctx);
    let stale = UncoveredRegexCaches::take();
    ctx.gc_collect_exact();
    stale.restore();
    assert_eq!(ctx.tagged_heap().tagged_heap_identity(), identity);
    // Box ownership checks inspect the allocation registry, never the freed
    // char-table/hash-table memory. Exact collection must have swept both.
    assert!(!ctx.tagged_heap().owns_heap_value_for_test(table));
    assert!(!ctx.tagged_heap().owns_heap_value_for_test(payload));
    match synchronize_at {
        SynchronizeAt::Activation => ctx.setup_thread_locals(),
        SynchronizeAt::RootEnumeration => {
            // Install the relocated heap pointer without activating regexp
            // caches: root enumeration must independently reject an old epoch.
            // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
            neovm_value::tagged::gc::set_tagged_heap(unsafe { ctx.tagged_heap_mut() });
        }
    }
    // Compare tagged words before GC can dereference a stale root. On the
    // unfixed source this assertion detects the reclaimed syntax/case table.
    let mut roots = Vec::new();
    collect_regex_gc_roots(
        &mut roots,
        ctx.tagged_heap().tagged_heap_identity(),
        ctx.tagged_heap().gc_collections(),
        neovm_value::tagged::gc::CacheRootScan::Collection,
    );
    assert!(
        roots.iter().all(|root| root.bits() != table.bits()),
        "regexp cache rooted a table swept by this Context on another thread"
    );
    ctx.setup_thread_locals();
    ctx.gc_collect_exact();
    assert_eq!(ctx.eval_str("(+ 20 22)").unwrap().as_fixnum(), Some(42));
}

pub(super) struct WarmRegexCaches {
    syntax: BufferSyntaxLookup,
    syntax_table: Value,
    translation_table: Value,
    search: Rc<CompiledPattern>,
    lisp_syntax: Rc<CompiledPattern>,
    lisp_translation: Rc<CompiledPattern>,
    literal_translation: Rc<CaseTranslation>,
}

impl WarmRegexCaches {
    pub(super) fn new(ctx: &mut Context) -> Self {
        neovm_eval::test_support::authority::enter_gc_inhibit_depth_for_test(ctx);
        let syntax_table = ctx
            .eval_str(
                "(progn (set-syntax-table (copy-syntax-table))
                        (modify-syntax-entry ?@ \"w\")
                        (syntax-table))",
            )
            .unwrap();
        let syntax = buffer_syntax_lookup(ctx.edit.buffers.current_buffer().unwrap());
        let pattern = LispString::from_utf8("[[:word:]]+");
        let CompiledSearchPattern::Emacs(search) =
            compile_search_pattern_with_posix(&pattern, false, false, &syntax).unwrap()
        else {
            panic!("syntax-dependent regexp used the literal path");
        };
        let lisp_syntax = compile_lisp_pattern_with_posix_translation(
            &pattern, false, false, true, None, &syntax,
        )
        .unwrap();
        assert!(search.used_syntax && lisp_syntax.used_syntax);
        ctx.eval_str("(set-syntax-table (standard-syntax-table))")
            .unwrap();

        let translation_table =
            neovm_text::emacs_core::chartable::make_char_table_value(Value::NIL, Value::NIL);
        neovm_text::emacs_core::chartable::builtin_set_char_table_range(
            vec![
                translation_table,
                Value::cons(Value::fixnum(b'A' as i64), Value::fixnum(b'B' as i64)),
                Value::fixnum(b'~' as i64),
            ],
            None,
        )
        .unwrap();
        let lisp_translation = compile_lisp_pattern_with_posix_translation(
            &LispString::from_utf8("gc-steady-translation[AB]+"),
            true,
            false,
            true,
            Some(translation_table),
            &DefaultSyntaxLookup,
        )
        .unwrap();
        let custom = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
        neovm_text::emacs_core::chartable::builtin_set_char_table_range(
            vec![
                custom,
                Value::fixnum(b'[' as i64),
                Value::fixnum(b']' as i64),
            ],
            None,
        )
        .unwrap();
        neovm_text::emacs_core::chartable::builtin_set_char_table_range(
            vec![
                custom,
                Value::fixnum('\u{03bb}' as i64),
                Value::fixnum(b'?' as i64),
            ],
            None,
        )
        .unwrap();
        crate::emacs_core::casetab_entries::builtin_set_case_table(ctx, vec![custom]).unwrap();
        let literal_translation =
            buffer_search_translation(ctx.edit.buffers.current_buffer().unwrap(), true).unwrap();
        neovm_eval::test_support::authority::leave_gc_inhibit_depth_for_test(ctx);
        Self {
            syntax,
            syntax_table,
            translation_table,
            search,
            lisp_syntax,
            lisp_translation,
            literal_translation,
        }
    }

    pub(super) fn cache_tables(&self) -> [Value; 3] {
        [
            self.syntax_table,
            self.translation_table,
            self.literal_translation.gc_root().unwrap(),
        ]
    }

    pub(super) fn assert_present_and_compiled_hits(&self, ctx: &Context) {
        // Holding the original Rc allocations makes an identity comparison
        // distinguish a cache hit from recompilation, even with address reuse.
        // Check membership before a lookup could read a reclaimed table.
        assert!(
            SEARCH_PATTERN_CACHE.with(|cache| cache.borrow().iter().any(|entry| {
                matches!(&entry.5, CompiledSearchPattern::Emacs(cp) if Rc::ptr_eq(cp, &self.search))
            })),
            "owning-thread collection discarded the search-pattern entry"
        );
        for original in [&self.lisp_syntax, &self.lisp_translation] {
            assert!(
                LISP_REGEX_PATTERN_CACHE.with(|cache| cache
                    .borrow()
                    .iter()
                    .any(|entry| Rc::ptr_eq(entry.compiled(), original))),
                "owning-thread collection discarded a Lisp-regexp entry"
            );
        }
        assert!(
            LITERAL_TRT_CACHE.with(|cache| cache
                .borrow()
                .as_ref()
                .is_some_and(|(_, trt)| { Rc::ptr_eq(trt, &self.literal_translation) })),
            "owning-thread collection discarded the literal translation"
        );
        // Guard all cache-held tables before compiled or literal lookups.
        // The minor liveness fixture also detaches the literal case table.
        for table in self.cache_tables() {
            assert!(ctx.tagged_heap().owns_heap_value_for_test(table));
        }
        let pattern = LispString::from_utf8("[[:word:]]+");
        let CompiledSearchPattern::Emacs(search) =
            compile_search_pattern_with_posix(&pattern, false, false, &self.syntax).unwrap()
        else {
            panic!("syntax-dependent regexp used the literal path");
        };
        assert!(
            Rc::ptr_eq(&search, &self.search),
            "search regexp recompiled"
        );
        let lisp = compile_lisp_pattern_with_posix_translation(
            &pattern,
            false,
            false,
            true,
            None,
            &self.syntax,
        )
        .unwrap();
        assert!(
            Rc::ptr_eq(&lisp, &self.lisp_syntax),
            "Lisp regexp recompiled"
        );
        let lisp = compile_lisp_pattern_with_posix_translation(
            &LispString::from_utf8("gc-steady-translation[AB]+"),
            true,
            false,
            true,
            Some(self.translation_table),
            &DefaultSyntaxLookup,
        )
        .unwrap();
        assert!(
            Rc::ptr_eq(&lisp, &self.lisp_translation),
            "translated regexp recompiled"
        );
    }

    pub(super) fn assert_present_and_hit(&self, ctx: &Context) {
        self.assert_present_and_compiled_hits(ctx);
        let literal =
            buffer_search_translation(ctx.edit.buffers.current_buffer().unwrap(), true).unwrap();
        assert!(Rc::ptr_eq(&literal, &self.literal_translation));
        assert_eq!(literal.translate(b'[' as u32), b']' as u32);
    }

    pub(super) fn assert_cached_search_results(&self, ctx: &Context) {
        // Guard every table dereference, including the syntax lookup's raw
        // identity and the literal translator's non-ASCII table lookup.
        for table in self.cache_tables() {
            assert!(ctx.tagged_heap().owns_heap_value_for_test(table));
        }
        for compiled in [&self.search, &self.lisp_syntax] {
            let (start, regs) = regex_emacs::re_search(compiled, b".@.", 0, 3, &self.syntax, 0)
                .expect("cache-only syntax table still classifies @ as word syntax");
            assert_eq!((start, regs.end[0]), (1, 2));
        }
        let text = b"..gc-steady-translation~~~";
        let (start, regs) = regex_emacs::re_search(
            &self.lisp_translation,
            text,
            0,
            text.len() as isize,
            &DefaultSyntaxLookup,
            0,
        )
        .expect("cache-only translation still folds [AB] to ~");
        assert_eq!((start, regs.end[0]), (2, text.len() as i64));
        let literal = LITERAL_TRT_CACHE.with(|cache| {
            let cache = cache.borrow();
            let (_, literal) = cache.as_ref().expect("literal cache remains populated");
            assert!(Rc::ptr_eq(literal, &self.literal_translation));
            literal.clone()
        });
        let matched = canon_fold_literal_find(b"..?..", "\u{03bb}".as_bytes(), true, &literal)
            .expect("cache-only literal translation still folds lambda to ?");
        assert_eq!((matched.start(), matched.end()), (2, 3));
    }
}

fn start_public_concurrent_cycle(ctx: &mut Context) {
    // The completed bootstrap makes the public safe-point path concurrent.
    assert!(!ctx.tagged_heap().tagged_heap_generational_enabled());
    assert!(ctx.tagged_heap().should_run_concurrent());
    for i in 0..20_000u64 {
        // SAFETY: test-only owner-thread heap setup through the non-swappable mutable accessor contract; the view is used in place for allocation/policy and is never swapped or moved across Worlds.
        unsafe { ctx.tagged_heap_mut() }.alloc_bignum(malachite::integer::Integer::from(
            (1u128 << 100) + i as u128,
        ));
    }
    ctx.set_gc_threshold(1);
    ctx.gc_safe_point();
    assert!(ctx.tagged_heap().concurrent_mark_running());
    assert!(ctx.tagged_heap().mark_in_progress());
    // Avoid the allocation cap and a second cycle while observing/draining it.
    ctx.set_gc_threshold(usize::MAX);
}

fn drive_public_cycle_until_sweeping(ctx: &mut Context) {
    for _ in 0..20_000 {
        if ctx.tagged_heap().sweep_in_progress() {
            return;
        }
        ctx.gc_safe_point();
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    panic!("public safe points did not start the deferred sweep");
}

#[derive(Clone, Copy)]
enum ForeignCollectionPhase {
    Marking,
    Sweeping,
}

fn assert_foreign_in_progress_activation_clears(phase: ForeignCollectionPhase) {
    let mut ctx = non_generational_context();
    ctx.gc_collect_exact();
    ctx.setup_thread_locals();
    let _warm = WarmRegexCaches::new(&mut ctx);
    let completed = ctx.tagged_heap().gc_collections();
    let stale = UncoveredRegexCaches::take();
    start_public_concurrent_cycle(&mut ctx);
    if matches!(phase, ForeignCollectionPhase::Sweeping) {
        drive_public_cycle_until_sweeping(&mut ctx);
    }
    stale.restore();
    assert_eq!(ctx.tagged_heap().gc_collections(), completed);
    assert!(ctx.tagged_heap().mark_in_progress() || ctx.tagged_heap().sweep_in_progress());
    ctx.setup_thread_locals();
    assert!(SEARCH_PATTERN_CACHE.with(|cache| cache.borrow().is_empty()));
    assert!(LISP_REGEX_PATTERN_CACHE.with(|cache| cache.borrow().is_empty()));
    assert!(LITERAL_TRT_CACHE.with(|cache| cache.borrow().is_none()));
    ctx.gc_collect_exact();
}

/// Saved owner-local caches absent from the collection's root publication.
/// Restoring their old coverage metadata preserves stale-cache regression
/// conditions while Context stays on the thread that constructs it.
#[must_use]
pub(super) struct UncoveredRegexCaches {
    _search: neovm_value::tls_scope::TlsScope<
        Vec<SearchPatternCacheEntry>,
        RefCell<Vec<SearchPatternCacheEntry>>,
    >,
    #[allow(clippy::vec_box)] // same address-stable MRU representation as TLS
    _lisp: neovm_value::tls_scope::TlsScope<
        Vec<Box<LispRegexPatternCacheEntry>>,
        RefCell<Vec<Box<LispRegexPatternCacheEntry>>>,
    >,
    _literal: neovm_value::tls_scope::TlsScope<
        Option<(usize, Rc<CaseTranslation>)>,
        RefCell<Option<(usize, Rc<CaseTranslation>)>>,
    >,
    _heap: neovm_value::tls_scope::TlsScope<usize, Cell<usize>>,
    _collection_epoch: neovm_value::tls_scope::TlsScope<Option<usize>, Cell<Option<usize>>>,
}

impl UncoveredRegexCaches {
    pub(super) fn take() -> Self {
        use neovm_value::tls_scope::TlsScope;
        Self {
            _search: TlsScope::new(&SEARCH_PATTERN_CACHE, Vec::new()),
            _lisp: TlsScope::new(&LISP_REGEX_PATTERN_CACHE, Vec::new()),
            _literal: TlsScope::new(&LITERAL_TRT_CACHE, None),
            _heap: TlsScope::restore(&REGEX_CACHE_HEAP, REGEX_CACHE_HEAP.with(Cell::get)),
            _collection_epoch: TlsScope::restore(
                &REGEX_CACHE_COLLECTION_EPOCH,
                REGEX_CACHE_COLLECTION_EPOCH.with(Cell::get),
            ),
        }
    }

    pub(super) fn restore(self) {
        drop(self);
    }
}

fn populate_cache(ctx: &mut Context, owner: CacheOwner) -> (Value, Value) {
    let payload = ctx.eval_str("(make-hash-table)").unwrap();
    let table = match owner {
        CacheOwner::SearchSyntax | CacheOwner::LispSyntax => {
            let table = ctx
                .eval_str("(progn (set-syntax-table (copy-syntax-table)) (syntax-table))")
                .unwrap();
            let syntax = BufferSyntaxLookup {
                syntax_table: neovm_text::emacs_core::syntax::SyntaxTable::for_buffer(
                    ctx.edit.buffers.current_buffer().unwrap(),
                ),
                category_table: None,
                word_boundary: Default::default(),
            };
            let pattern = LispString::from_utf8("[[:word:]]");
            match owner {
                CacheOwner::SearchSyntax => {
                    let compiled =
                        compile_search_pattern_with_posix(&pattern, false, false, &syntax).unwrap();
                    assert!(matches!(
                        compiled,
                        CompiledSearchPattern::Emacs(ref compiled) if compiled.used_syntax
                    ));
                }
                CacheOwner::LispSyntax => {
                    assert!(
                        compile_lisp_pattern_with_posix_translation(
                            &pattern, false, false, true, None, &syntax,
                        )
                        .unwrap()
                        .used_syntax
                    );
                }
                _ => unreachable!(),
            }
            ctx.eval_str("(set-syntax-table (standard-syntax-table))")
                .unwrap();
            table
        }
        CacheOwner::LispTranslation => {
            let table =
                neovm_text::emacs_core::chartable::make_char_table_value(Value::NIL, Value::NIL);
            compile_lisp_pattern_with_posix_translation(
                &LispString::from_utf8("gc-collection-epoch-translation"),
                true,
                false,
                true,
                Some(table),
                &DefaultSyntaxLookup,
            )
            .unwrap();
            table
        }
        CacheOwner::LiteralTranslation => {
            let custom = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
            neovm_text::emacs_core::chartable::builtin_set_char_table_range(
                vec![
                    custom,
                    Value::fixnum(b'[' as i64),
                    Value::fixnum(b']' as i64),
                ],
                None,
            )
            .unwrap();
            crate::emacs_core::casetab_entries::builtin_set_case_table(ctx, vec![custom]).unwrap();
            let buffer = ctx.edit.buffers.current_buffer().unwrap();
            let table = neovm_text::emacs_core::casetab::buffer_case_canon_table(buffer).unwrap();
            assert_eq!(
                buffer_search_translation(buffer, true)
                    .unwrap()
                    .translate(b'[' as u32),
                b']' as u32
            );
            ctx.eval_str("(set-case-table (standard-case-table))")
                .unwrap();
            table
        }
    };
    neovm_text::emacs_core::chartable::builtin_set_char_table_range(
        vec![table, Value::fixnum(0x1ffff), payload],
        None,
    )
    .unwrap();
    assert!(
        regex_roots(ctx)
            .iter()
            .any(|root| root.bits() == table.bits())
    );
    (table, payload)
}

fn regex_roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_regex_gc_roots(
        &mut roots,
        ctx.tagged_heap().tagged_heap_identity(),
        ctx.tagged_heap().gc_collections(),
        neovm_value::tagged::gc::CacheRootScan::snapshot(
            ctx.tagged_heap().mark_in_progress() || ctx.tagged_heap().sweep_in_progress(),
        ),
    );
    roots
}
