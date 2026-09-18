//! A symbol id must never be recovered from a NAME on a hot path.
//!
//! `Vm::builtin_name_id(name)` is
//! `lookup_interned(name).unwrap_or_else(|| intern(name))` -- a global-interner
//! `RwLock` acquisition and a string hash. Paying that per call, for an id the
//! caller already had or that is fixed at compile time, has been found and
//! fixed FOUR separate times in `runtime/bytecode/vm.rs`:
//!
//! * `Op::VarRef`/`Op::VarSet` -- see the comment above `sym_id_at`
//! * `Op::CallBuiltin` -- see `vm_special_builtin_ids`
//! * the arithmetic opcodes (`74ffad169`)
//! * `aset_for_jit` and the JIT builtin entries (`30292177e`)
//! * `Op::Aset`'s redefinition probe -- it passed the LITERAL "aset" to a
//!   helper that resolved it, so the literal check below could not see it
//!
//! The first three were each fixed by someone who wrote down the lesson, and
//! it came back anyway. This is that lesson as a test.
//!
//! The fifth got past the literal check because the name travelled through a
//! `&str` PARAMETER, so the count pin below covers the other half: any new
//! resolver call site in the VM has to be argued for.

macro_rules! hot_dispatch_sources {
    ($($path:literal),+ $(,)?) => {
        [$(($path, include_str!(concat!("../../", $path)))),+]
    };
}

/// Which files this guard covers: the paths every Lisp call flows through.
/// A cold path may legitimately resolve a name (charset and coding-system
/// setup do), which is why this is scoped rather than crate-wide -- a lint
/// that fires on correct code gets switched off.
fn hot_dispatch_files() -> [(&'static str, &'static str); 4] {
    hot_dispatch_sources![
        "runtime/bytecode/vm.rs",
        "runtime/eval/apply.rs",
        "runtime/jit/compile/dispatch.rs",
        "runtime/eval/tree_walk.rs",
    ]
}

#[test]
fn hot_dispatch_never_interns_a_name_known_at_compile_time() {
    // A string LITERAL means the symbol is known when the code is compiled, so
    // resolving it at run time is never necessary: cache it in a `OnceLock`
    // (`Vm::cached_builtin_id`) instead. `aset_for_jit` did exactly this and
    // cost `dhrystone` ~11% of its run.
    for (path, source) in hot_dispatch_files() {
        for forbidden in [
            concat!("builtin_name", "_id(\""),
            concat!("lookup_", "interned(\""),
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} resolves a symbol from a LITERAL name via `{forbidden}...`.\n\
                 The id is known at compile time -- cache it with\n\
                 `Vm::cached_builtin_id(\"name\", &NAME_ID)` and a `OnceLock` static."
            );
        }
    }
}

#[test]
fn only_the_vm_special_fallback_round_trips_an_id_through_its_name() {
    // `dispatch_vm_builtin` takes a `&str` and ends in `builtin_name_id`, so
    // handing it `resolve_sym(id)` is a complete `SymId -> &str -> SymId` round
    // trip through the global interner. Exactly ONE site may do it: the
    // fallback in `dispatch_vm_builtin_id` for the thirteen VM-special
    // builtins, which are the only ones matched as strings.
    //
    // Pinned as a count rather than forbidden outright so that adding a second
    // one is a deliberate act with a test to update, not an accident.
    let (path, source) = hot_dispatch_files()[0];
    let round_trips = source
        .matches(concat!("dispatch_vm_builtin(resolve_", "sym("))
        .count();
    assert_eq!(
        round_trips, 1,
        "{path} has {round_trips} `SymId -> name -> SymId` round trips; exactly one \
         is sanctioned (the VM-special fallback in `dispatch_vm_builtin_id`).\n\
         Callers that already hold a `SymId` must use `dispatch_vm_builtin_id`."
    );
}

#[test]
fn the_vm_resolves_a_builtin_name_on_exactly_one_path() {
    // The literal check above cannot see a name that reaches the resolver
    // through a parameter -- which is how `Op::Aset` paid an interner lookup
    // per opcode for years: it called
    // `maybe_call_named_function_cell(func, "aset", ..)` and the helper did the
    // `builtin_name_id`. So pin the number of CALL sites too; a new one is then
    // a deliberate act with a test to update.
    //
    // Sanctioned: exactly one CALL, `call_named_builtin`'s by-name sibling,
    // documented as the cold entry for callers that genuinely only have a name.
    // Matched in the `Self::`-qualified form so the definition and the doc
    // comments that name it do not count.
    let (path, source) = hot_dispatch_files()[0];
    let uses = source
        .matches(concat!("Self::builtin_name", "_id("))
        .count();
    assert_eq!(
        uses, 1,
        "{path} has {uses} `Self::builtin_name_id(` call sites; exactly one is \
         sanctioned (the documented by-name sibling of `call_named_builtin`).\n\
         A hot path must take the `SymId` -- see `Vm::cached_builtin_id`."
    );
}
