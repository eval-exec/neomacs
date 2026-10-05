//! A compiler worker's preceding leaf cannot supply a later leaf's CLIF census.
//! Threading: each test owns its context, leaves and scalar compiler override.

use super::super::compile_pipeline_tests::{captured_clif, function};
use super::*;

#[derive(Clone, Copy, Debug, strum::IntoStaticStr)]
enum RootCountField {
    #[strum(serialize = "rw_stores=")]
    Emitted,
    #[strum(serialize = "rw_elided=")]
    Elided,
}

#[derive(Debug, PartialEq, Eq)]
struct RootCounts {
    emitted: u32,
    elided: u32,
}

impl RootCounts {
    const ZERO: Self = Self {
        emitted: 0,
        elided: 0,
    };

    fn from_clif(clif: &str) -> Self {
        let header = clif.lines().next().expect("the emitted leaf has a header");
        let field = |name: RootCountField| {
            let prefix: &'static str = name.into();
            header
                .split_whitespace()
                .find_map(|part| part.strip_prefix(prefix))
                .expect("root-window diagnostics are present")
                .parse()
                .expect("root-window diagnostics are counts")
        };
        Self {
            emitted: field(RootCountField::Emitted),
            elided: field(RootCountField::Elided),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum NativeEntry {
    Normal,
    OsrLoopHeader,
}

impl NativeEntry {
    fn osr_pc(self) -> Option<usize> {
        match self {
            Self::Normal => None,
            Self::OsrLoopHeader => Some(0),
        }
    }
}

fn lower(f: &ByteCodeFunction, entry: NativeEntry) -> (CompiledLeaf, RootCounts) {
    let mut leaf = None;
    let clif = captured_clif(|| {
        leaf = Some(
            lower_leaf_full_osr(
                f.executable_ops(),
                &f.constants,
                f.params.required.len(),
                f.executable_gnu_byte_offset_map(),
                None,
                entry.osr_pc(),
                0,
            )
            .expect("the real baseline emitter accepts this body"),
        );
    });
    assert_eq!(clif.len(), 1, "exactly one completed leaf is emitted");
    (
        leaf.expect("captured native leaf"),
        RootCounts::from_clif(&clif[0]),
    )
}

fn seed_root_stores() -> CompiledLeaf {
    // Two generic calls force the real hoisted window and a nonzero census.
    // Keep the leaf alive while compiling its successor on the same thread.
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(1),
            Op::Add,
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(1),
            Op::Add,
            Op::Return,
        ],
        vec![Value::make_int(1)],
        2,
    );
    let (leaf, counts) = lower(&f, NativeEntry::Normal);
    assert!(
        counts.emitted > 0,
        "preceding leaf emits actual root stores"
    );
    leaf
}

fn countdown() -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::Goto(0),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        1,
    )
}

fn assert_successor_census(entry: NativeEntry) {
    let _mode = opt_mode_scope_for_test(OptMode::Legacy);
    let mut ctx = Context::new();
    let _rooted = seed_root_stores();
    let f = countdown();
    let (leaf, counts) = lower(&f, entry);
    assert_eq!(counts, RootCounts::ZERO, "this leaf has no rooting sites");
    let args = [Value::make_int(41)];
    let expected = {
        let mut vm = Vm::from_context(&mut ctx);
        vm.force_interpreter_only_for_test();
        vm.execute(&f, args.to_vec()).expect("Tier-0 answer")
    };
    let NativeRun::Ok(bits) = leaf.call_consts(
        &mut ctx as *mut Context as *mut u8,
        f.constants.as_ptr(),
        &args,
    ) else {
        panic!("the successor must finish natively")
    };
    assert_eq!(Value::from_bits(bits), expected);
}

#[test]
fn legacy_baseline_census_does_not_inherit_preceding_root_stores() {
    assert_successor_census(NativeEntry::Normal);
}

#[test]
fn legacy_osr_census_does_not_inherit_preceding_root_stores() {
    assert_successor_census(NativeEntry::OsrLoopHeader);
}
