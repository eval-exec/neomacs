//! Native epoch witnesses, cold rematerialization, and exact serviced-poll replay.
//! Hand-assembled controls compare to Tier-0; Lisp-visible oracle cases are in
//! switch_dispatch tests. Compiler settings and roots belong to this test only.

use super::*;
use crate::emacs_core::jit::compile::knobs::{
    force_inline_switch_for_test, force_switch_hoist_for_test,
};
use crate::emacs_core::jit::opt::passes::switch_epochs::SwitchEpochGuards;
use crate::emacs_core::value::HashTableTest;

#[must_use = "the compiler overrides end when this guard is dropped"]
struct SwitchSettings {
    _settings: Settings,
    _thread: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(SwitchSettings: Send, Sync);
impl SwitchSettings {
    fn enter() -> Self {
        let settings = Settings::enter();
        force_inline_switch_for_test(Some(true));
        force_switch_hoist_for_test(Some(true));
        Self {
            _settings: settings,
            _thread: std::marker::PhantomData,
        }
    }
}
impl Drop for SwitchSettings {
    fn drop(&mut self) {
        force_inline_switch_for_test(None);
        force_switch_hoist_for_test(None);
    }
}

fn put(table: Value, key: Value, target: usize) {
    table
        .with_hash_table_mut(|ht| {
            ht.insert(key.to_hash_key(&ht.test), key, Value::fixnum(target as i64));
        })
        .unwrap();
}

/// Two-argument countdown/count loop. The switch maps key `a` to an
/// incrementing arm, `b` to a no-op arm; mutating a -> b changes the count.
fn source() -> ByteCodeFunction {
    let table = Value::hash_table(HashTableTest::Eq);
    put(table, Value::symbol("a"), 11);
    put(table, Value::symbol("b"), 14);
    function(
        vec![
            Op::Goto(1),
            Op::StackRef(1), // header [n acc n]
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(18),
            Op::Constant(1), // a
            Op::Constant(2),
            Op::Switch,
            Op::Goto(14),
            Op::Nil,
            Op::Pop, // miss
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1), // a -> acc++
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::fixnum(0), Value::symbol("a"), table],
        2,
    )
}

/// Exercise the same representation pipeline as a production Opt leaf.
fn plan_entry(source: &ByteCodeFunction, osr_pc: Option<usize>) -> ir::Func {
    let arity = source.params.stack_shape().unwrap().required();
    let cfg = analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    opt_backend::build_plan(
        source.executable_ops(),
        &source.constants,
        &cfg,
        ir::ParamShape {
            required: arity,
            ..Default::default()
        },
        0,
        osr_pc,
    )
    .unwrap()
}

fn plan(source: &ByteCodeFunction) -> ir::Func {
    plan_entry(source, None)
}

fn switch_sites(func: &ir::Func) -> HashSet<ir::Block> {
    func.blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            matches!(b.term, ir::Term::Switch { .. }).then_some(ir::Block(i as u32))
        })
        .collect()
}

#[test]
fn opt_switch_hoist_schedules_entry_and_every_poll_but_declines_reentry() {
    let _settings = SwitchSettings::enter();
    let f = source();
    let mut ir = plan(&f);
    let sites = switch_sites(&ir);
    let schedule = SwitchEpochGuards::build(&ir, &sites).unwrap();
    assert_eq!(sites.len(), 1);
    assert!(
        sites.iter().all(|&site| {
            crate::emacs_core::jit::opt::passes::switch_epochs::deferred_table_const(&ir, site)
                .is_some()
        }),
        "the sole table use is rematerialized inside the shim block"
    );
    assert!(sites.iter().all(|&site| schedule.is_hoisted(site)));
    let polls: Vec<_> = ir
        .insts
        .iter()
        .enumerate()
        .filter(|(_, i)| i.op == ir::Opcode::Poll)
        .map(|(i, _)| ir::Inst(i as u32))
        .collect();
    assert!(!polls.is_empty());
    assert!(polls.iter().all(|&id| schedule.after_poll(id).len() == 1));
    drop(schedule);
    // A possible Lisp call, including an allocating reentrant slow path,
    // kills the proof regardless of how an opaque opcode is named.
    let id = ir
        .blocks
        .iter()
        .find(|b| matches!(b.term, ir::Term::Switch { .. }))
        .unwrap()
        .insts[0];
    for effects in [
        Effects::MAY_REENTER,
        Effects::MAY_GC,
        Effects::WRITE_HEAP,
        Effects::WRITE_BINDINGS,
    ] {
        ir.insts[id.index()].eff = effects;
        let schedule = SwitchEpochGuards::build(&ir, &sites).unwrap();
        assert!(sites.iter().all(|&site| !schedule.is_hoisted(site)));
    }
}

#[test]
fn opt_switch_hoist_stale_entry_resumes_before_dispatch() {
    let _settings = SwitchSettings::enter();
    let mut ctx = Context::new_minimal_vm_harness();
    let f = source();
    let _roots = Roots::new(&f.constants);
    let ir = plan(&f);
    let leaf = lower(&f, &ir);
    assert!(
        leaf.reloc_values().contains(&f.constants[2]),
        "the immediate table must remain rooted by its leaf"
    );
    let args = [Value::fixnum(3), Value::fixnum(0)];
    let expected = tier0(&mut ctx, &f, &args).unwrap();
    assert_eq!(print_value(&expected), gnu_control_expect(0));
    assert_eq!(native_ok(&mut ctx, &leaf, &args), expected);
    put(f.constants[2], Value::symbol("a"), 14);
    let expected = tier0(&mut ctx, &f, &args).unwrap();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &args) else {
        panic!("stale preheader must exit");
    };
    assert_eq!(resume.pc, 1);
    assert_eq!(resume.stack, args);
    let answer = resumed(&mut ctx, &f, &resume).unwrap();
    assert_eq!(answer, expected);
    assert_eq!(print_value(&answer), gnu_control_expect(1));
}

#[test]
fn opt_switch_hoist_service_poll_mutation_resumes_current_stack() {
    let _settings = SwitchSettings::enter();
    let mut ctx = Context::new();
    let f = source();
    let _roots = Roots::new(&f.constants);
    let leaf = lower(&f, &plan(&f));
    ctx.obarray
        .set_symbol_value("switch-hoist-table", f.constants[2]);
    ctx.eval_str("(setq post-gc-hook (list (lambda () (setq post-gc-hook nil) (puthash 'a 14 switch-hoist-table))))").unwrap();
    ctx.gc_stress = true;
    reset_bytecode_branch_poll_count();
    let result = leaf.call(
        &mut ctx as *mut Context as *mut u8,
        &[Value::fixnum(300), Value::fixnum(0)],
    );
    ctx.gc_stress = false;
    assert_eq!(bytecode_branch_poll_count(), 1);
    let NativeRun::DeoptAt(resume) = result else {
        panic!("poll mutation must invalidate witness: {result:?}");
    };
    assert_eq!(resume.pc, 1);
    // Check the actual post-edge state, so a restart/replay cannot hide.
    assert_eq!(resume.stack, [Value::fixnum(45), Value::fixnum(255)]);
    assert_eq!(ctx.jit_root_stack_top, 0);
    let expected = tier0(&mut ctx, &f, &resume.stack).unwrap();
    let answer = resumed(&mut ctx, &f, &resume).unwrap();
    assert_eq!(answer, expected);
    assert_eq!(print_value(&answer), gnu_control_expect(2));
}

#[test]
fn opt_switch_hoist_osr_stale_entry_checks_the_interpreter_stack() {
    let _settings = SwitchSettings::enter();
    let mut ctx = Context::new();
    let f = source();
    let _roots = Roots::new(&f.constants);
    let ir = plan_entry(&f, Some(1));
    let sites = switch_sites(&ir);
    let schedule = SwitchEpochGuards::build(&ir, &sites).unwrap();
    assert!(sites.iter().all(|&site| schedule.is_hoisted(site)));
    let leaf = lower(&f, &ir);
    put(f.constants[2], Value::symbol("a"), 14);
    let args = [Value::fixnum(7), Value::fixnum(99)];
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &args) else {
        panic!("stale OSR entry must exit");
    };
    assert_eq!(resume.pc, 1);
    assert_eq!(resume.stack, args);
    assert_eq!(
        resumed(&mut ctx, &f, &resume).unwrap(),
        tier0(&mut ctx, &f, &args).unwrap()
    );
    assert_eq!(
        print_value(&resumed(&mut ctx, &f, &resume).unwrap()),
        gnu_control_expect(3)
    );
}

#[test]
fn opt_switch_hoist_off_and_inline_switch_off_preserve_clif() {
    let _settings = SwitchSettings::enter();
    let f = source();
    let ir = plan(&f);
    force_inline_switch_for_test(Some(false));
    let on = captured_clif(|| {
        let _ = lower(&f, &ir);
    });
    force_switch_hoist_for_test(Some(false));
    let off = captured_clif(|| {
        let _ = lower(&f, &ir);
    });
    assert!(!on.is_empty());
    assert_eq!(
        on.iter().map(|s| normalize(s)).collect::<Vec<_>>(),
        off.iter().map(|s| normalize(s)).collect::<Vec<_>>()
    );
}

fn normalize(text: &str) -> String {
    // CLIF repeats constants in operand comments. Mask pointer-sized Linux
    // user addresses there as well, preserving offsets and integer limits.
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("0x") {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = 2 + rest[2..]
            .bytes()
            .take_while(|b| b.is_ascii_hexdigit() || *b == b'_')
            .count();
        let digits = rest[2..end].replace('_', "");
        let pointer = u64::from_str_radix(&digits, 16)
            .is_ok_and(|value| ((1u64 << 32)..(1u64 << 48)).contains(&value));
        output.push_str(if pointer { "0xADDR" } else { &rest[..end] });
        rest = &rest[end..];
    }
    output.push_str(rest);
    output
}

const MUTATION_SETUP: &str = r#"(progn
 (require 'cl-lib)
 (defvar switch-hoist-ticks 0)
 (defvar switch-hoist-table nil)
 (defun switch-hoist-mutate ()
   (setq switch-hoist-ticks (1+ switch-hoist-ticks))
   (when (= switch-hoist-ticks 3)
     (puthash 'a (gethash 'b switch-hoist-table) switch-hoist-table)))
 (setq switch-hoist-function
   (byte-compile
    (lambda (x n)
      (let ((acc 0))
        (while (> n 0)
          (switch-hoist-mutate)
          (setq acc (+ acc (pcase x ('a 1) ('b 10) ('c 20) ('d 30) ('e 40) ('f 50) (_ 0))))
          (setq n (1- n)))
        acc))))
 (setq switch-hoist-ticks 0
       switch-hoist-table
       (cl-find-if #'hash-table-p (append (aref switch-hoist-function 2) nil))))"#;

fn gnu_fixture(name: &str, form: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join(format!(
        "src/emacs_core/runtime/jit/compile/tests/switch_hoist_{name}.expect"
    ));
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root.join(format!("../../tmp/switch-hoist-{name}-oracle.el"));
        std::fs::write(
            &script,
            format!(";;; -*- lexical-binding: t; -*-\n(prin1 {form})\n"),
        )
        .unwrap();
        let sandbox = root
            .ancestors()
            .map(|path| path.join("tmp/v10x/sandbox-run.sh"))
            .find(|path| path.is_file())
            .expect("GNU refresh requires the campaign sandbox-run.sh");
        let emacs = crate::test_utils::gnu_oracle().executable_path();
        let output = std::process::Command::new(sandbox)
            .args(["timeout", "30"])
            .arg(emacs)
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "GNU: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(&fixture, &output.stdout).unwrap();
    }
    std::fs::read_to_string(fixture)
        .expect("refresh GNU fixture with UPDATE_EXPECT=1")
        .trim_end()
        .to_owned()
}

fn gnu_mutation_expect() -> String {
    gnu_fixture(
        "call",
        &format!("(progn {MUTATION_SETUP} (funcall switch-hoist-function 'a 300))"),
    )
}

/// GNU executes the same control loop on each materialized continuation.
/// Poll cadence itself is a native protocol assertion, not a GNU expectation.
fn gnu_control_expect(index: usize) -> String {
    let text = gnu_fixture(
        "control",
        r#"(progn
      (require 'cl-lib)
      (let* ((f (byte-compile (lambda (x n acc)
                   (while (> n 0)
                     (setq acc (+ acc (pcase x ('a 1) ('b 0) ('c 0) ('d 0) ('e 0) ('f 0) (_ 0))))
                     (setq n (1- n)))
                   acc)))
             (table (cl-find-if #'hash-table-p (append (aref f 2) nil)))
             (initial (funcall f 'a 3 0)))
        (puthash 'a (gethash 'b table) table)
        (list initial (funcall f 'a 3 0) (funcall f 'a 45 255) (funcall f 'a 7 99))))"#,
    );
    text.trim_matches(['(', ')'])
        .split_whitespace()
        .nth(index)
        .expect("GNU control answer")
        .to_owned()
}

#[test]
fn opt_switch_hoist_call_mutation_matches_gnu() {
    let _settings = SwitchSettings::enter();
    let expected = gnu_mutation_expect();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(&format!("(eval '{MUTATION_SETUP} t)"))
        .unwrap();
    let f = ctx.eval_str("switch-hoist-function").unwrap();
    let _roots = Roots::new(&[f]);
    let data = f.get_bytecode_data().unwrap();
    let ir = plan(data);
    let sites = switch_sites(&ir);
    assert!(!sites.is_empty());
    let schedule = SwitchEpochGuards::build(&ir, &sites).unwrap();
    assert!(
        sites.iter().all(|&s| !schedule.is_hoisted(s)),
        "the call must block hoisting"
    );
    let leaf = lower(data, &ir);
    let args = [Value::symbol("a"), Value::fixnum(300)];
    let actual = match leaf.call(&mut ctx as *mut Context as *mut u8, &args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        NativeRun::DeoptAt(resume) => resumed(&mut ctx, data, &resume).unwrap(),
        other => panic!("native call mutation: {other:?}"),
    };
    assert_eq!(print_value(&actual), expected);
}

#[test]
fn opt_switch_hoist_symbols_with_pos_miss_still_uses_lookup() {
    let _settings = SwitchSettings::enter();
    let mut ctx = Context::new();
    let mut f = source();
    // Replace the key constant with an argument, keeping a rooted static table.
    f.ops[5] = Op::StackRef(2);
    f.params = crate::emacs_core::value::LambdaParams {
        required: vec![
            crate::emacs_core::intern::intern("swp-key"),
            crate::emacs_core::intern::intern("swp-n"),
            crate::emacs_core::intern::intern("swp-acc"),
        ],
        optional: vec![],
        rest: None,
    }
    .into();
    f.seal_hand_assembled_ops();
    let key = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("a"), Value::fixnum(9));
    let _roots = Roots::new(&[key, f.constants[2]]);
    let leaf = lower(&f, &plan(&f));
    for swp in [false, true] {
        ctx.symbols_with_pos_enabled = swp;
        let args = [key, Value::fixnum(3), Value::fixnum(0)];
        let expected = tier0(&mut ctx, &f, &args).unwrap();
        assert_eq!(native_ok(&mut ctx, &leaf, &args), expected);
    }
}

#[test]
fn opt_switch_hoist_covers_both_sibling_backedge_polls() {
    let _settings = SwitchSettings::enter();
    let mut f = source();
    f.ops[4] = Op::GotoIfNil(22);
    f.ops.splice(
        17..,
        [
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNotNil(1),
            Op::Goto(1),
            Op::Return,
        ],
    );
    f.seal_hand_assembled_ops();
    let ir = plan(&f);
    let sites = switch_sites(&ir);
    let schedule = SwitchEpochGuards::build(&ir, &sites).unwrap();
    assert!(sites.iter().all(|&s| schedule.is_hoisted(s)));
    let polls: Vec<_> = ir
        .insts
        .iter()
        .enumerate()
        .filter(|(_, i)| i.op == ir::Opcode::Poll)
        .map(|(i, _)| ir::Inst(i as u32))
        .collect();
    assert_eq!(polls.len(), 2);
    assert!(
        polls
            .iter()
            .all(|&poll| schedule.after_poll(poll).len() == 1)
    );
}
