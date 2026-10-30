//! GNU-backed weak-key parity across the four weakness kinds and the
//! key-type matrix, in both generation modes; plus image-resident permanence
//! through a real pdump load.
//!
//! GNU 31.1 truth, sandboxed with three byte-identical refreshes per case
//! (`tmp/weakkey/STEP0-REPORT.md`, raw outputs under
//! `tmp/weakkey/step0-mine/`), full collection while the key is rooted:
//!
//! | weakness | rooted/dropped1/dropped2 |
//! |---|---|
//! | `key`, `key-and-value` | 1/0/0 |
//! | `value`, `key-or-value` | 1/1/1 (the table's strong side keeps the key) |
//!
//! for vector, string, record, bool-vector, bytecode, lambda without free
//! vars, lambda with free vars, eval-fresh lambda, marker and a cons control,
//! under lexical-binding t and nil.

use crate::test_utils::{runtime_startup_context, with_legacy_gc};

/// GNU truth per weakness kind; the probe prints `rooted d1 d2`.
const KINDS: &[(&str, [i64; 3])] = &[
    ("key", [1, 0, 0]),
    ("key-and-value", [1, 0, 0]),
    ("value", [1, 1, 1]),
    ("key-or-value", [1, 1, 1]),
];

const MAKERS: &[(&str, &str)] = &[
    ("vector", "(make-vector 3 0)"),
    ("string", "(make-string 8 ?x)"),
    ("record", "(record 'weakkey-record 0 0)"),
    ("bool-vector", "(make-bool-vector 9 nil)"),
    ("byte-code", "(make-byte-code 0 \"\\300\\207\" [1] 1)"),
    ("lambda-no-free", "(lambda () 1)"),
    (
        "lambda-free",
        "(let ((weakkey-capture (make-vector 1 42))) (lambda () weakkey-capture))",
    ),
    ("eval-fresh-lambda", "(eval '(lambda () 1))"),
    ("marker", "(make-marker)"),
    ("cons", "(list 1 2)"),
];

/// Pin NEOVM_GC_GENERATIONAL for the context creation, restoring it on drop
/// (nextest gives each test its own process; the evaluator is created after
/// the variable is set).
struct GenerationalEnv(Option<std::ffi::OsString>);

impl GenerationalEnv {
    fn set(value: &str) -> Self {
        let previous = std::env::var_os("NEOVM_GC_GENERATIONAL");
        // SAFETY: the test process is single-threaded until nextest starts
        // other tests, and this restores the caller's value on drop.
        unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", value) };
        Self(previous)
    }
}

impl Drop for GenerationalEnv {
    fn drop(&mut self) {
        // SAFETY: see `set`.
        unsafe {
            match self.0.take() {
                Some(previous) => std::env::set_var("NEOVM_GC_GENERATIONAL", previous),
                None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
            }
        }
    }
}

fn probe(ev: &mut crate::emacs_core::eval::Context, weakness: &str, maker: &str) -> [i64; 3] {
    // Globals + one progn per step (the shape whose GNU truth is recorded in
    // tmp/weakkey/step0-mine/gnu-weakness-s2*.tsv): allocate and root the key,
    // run a full collection while it is rooted, drop the root, then two more
    // full collections. A `let` that is the final form of its own top-level
    // eval triggers an unrelated evaluator temp-root residue (reported
    // separately); this probe avoids it.
    ev.eval_str(&format!(
        "(progn (setq wk-tbl (make-hash-table :weakness '{weakness}) wk-keep nil) \
         (setq wk-keep {maker}) (puthash wk-keep 1 wk-tbl))"
    ))
    .expect("weak-key allocation");
    ev.eval_str("(garbage-collect)")
        .expect("rooted full collection");
    ev.eval_str("(setq rooted (hash-table-count wk-tbl))")
        .expect("rooted count");
    ev.eval_str("(setq wk-keep nil)").expect("drop root");
    ev.eval_str("(garbage-collect)")
        .expect("first unrooted collection");
    ev.eval_str("(setq d1 (hash-table-count wk-tbl))")
        .expect("first dropped count");
    ev.eval_str("(garbage-collect)")
        .expect("second unrooted collection");
    ev.eval_str("(setq d2 (hash-table-count wk-tbl))")
        .expect("second dropped count");
    ["rooted", "d1", "d2"].map(|name| {
        ev.variable_value_by_name(name)
            .and_then(|v| v.as_fixnum())
            .unwrap_or(-1)
    })
}

fn run_matrix(ev: &mut crate::emacs_core::eval::Context) {
    for &(weakness, expected) in KINDS {
        for &(maker_name, maker) in MAKERS {
            for lexical in [false, true] {
                ev.set_lexical_binding(lexical);
                let got = probe(ev, weakness, maker);
                assert_eq!(
                    got, expected,
                    "weakness={weakness} key={maker_name} lexical={lexical}: \
                     GNU 31.1 gives {expected:?}",
                );
            }
        }
    }
}

#[test]
fn weak_key_matrix_matches_gnu_gen1() {
    let _env = GenerationalEnv::set("1");
    let mut ev = runtime_startup_context();
    run_matrix(&mut ev);
}

#[test]
fn weak_key_matrix_matches_gnu_gen0() {
    let mut ev = with_legacy_gc(runtime_startup_context);
    run_matrix(&mut ev);
}

/// A real pdump load stamps every heap object reconstructed from an image
/// row; the first partition cycle must make exactly those permanent while
/// leaving the rest of the heap ordinary. They then survive full collections
/// with no root.
#[test]
fn pdump_reconstructed_rows_become_permanent_at_the_first_partition_cycle() {
    let path = std::env::temp_dir().join(format!(
        "neovm-weakkey-image-permanence-{}.pdump",
        std::process::id()
    ));
    {
        let ev = runtime_startup_context();
        crate::emacs_core::pdump::dump_to_file(&ev, &path).expect("scratch dump");
    }
    let mut ev = crate::emacs_core::pdump::load_from_dump(&path).expect("scratch reload");
    crate::emacs_core::builtins::init_builtins(&mut ev);
    let _ = std::fs::remove_file(&path);
    assert!(
        ev.tagged_heap.dump_partition_active(),
        "a loaded image activates the partition",
    );

    let flagged: Vec<usize> = ev
        .tagged_heap
        .non_cons_object_addrs
        .iter()
        .copied()
        .filter(|&addr| {
            // SAFETY: every address in the owned catalog names initialized
            // header storage until it is freed; nothing runs concurrently.
            let header = unsafe { &*(addr as *const crate::tagged::header::GcHeader) };
            header.flags.image_resident()
        })
        .collect();
    assert!(
        !flagged.is_empty(),
        "the loader reconstructs dump rows (e.g. hash tables) as heap objects",
    );
    for &addr in &flagged {
        let header = unsafe { &*(addr as *const crate::tagged::header::GcHeader) };
        assert!(
            !header.generation.permanent(),
            "permanence is established by the first partition cycle, not at load",
        );
    }

    ev.gc_collect_exact();

    for &addr in &flagged {
        assert!(
            ev.tagged_heap.non_cons_object_addrs.contains(&addr),
            "image resident {addr:#x} was reclaimed by its own partition cycle",
        );
        let header = unsafe { &*(addr as *const crate::tagged::header::GcHeader) };
        assert!(header.flags.image_resident());
        assert!(header.tenured);
        assert!(
            header.generation.permanent(),
            "image resident {addr:#x} must become permanent",
        );
        assert!(header.black_by_generation(crate::tagged::header::CollectionScope::Full));
    }
    // A later full collection keeps them: they are the mapped image's
    // permanent equivalent, not ordinary old objects.
    ev.gc_collect_exact();
    for &addr in &flagged {
        assert!(ev.tagged_heap.non_cons_object_addrs.contains(&addr));
        let header = unsafe { &*(addr as *const crate::tagged::header::GcHeader) };
        assert!(header.generation.permanent());
    }
}
