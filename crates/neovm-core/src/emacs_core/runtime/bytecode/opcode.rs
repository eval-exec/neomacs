//! Bytecode opcodes for the neovm bytecode compiler and VM.
//!
//! Uses a high-level `Op` enum where each variant carries its operands.
//! This is easier to work with during compilation. A future optimization
//! pass can serialize to a compact byte stream.

use serde::{Deserialize, Serialize};

/// A single bytecode instruction.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op {
    // -- Constants and stack --------------------------------------------------
    /// Push a constant from the constant pool.
    Constant(u16),
    /// Push nil.
    Nil,
    /// Push t.
    True,
    /// Pop and discard top of stack.
    Pop,
    /// Duplicate top of stack.
    Dup,
    /// Reference stack slot (0 = TOS, 1 = below TOS, ...).
    StackRef(u16),
    /// Assign TOS into stack slot N below TOS, then pop TOS.
    /// N = 0 behaves like pop.
    StackSet(u16),
    /// Emacs `discardN` semantics.
    /// Low 7 bits: number of values to discard.
    /// High bit: preserve original TOS in the last kept slot before discard.
    DiscardN(u8),

    // -- Variable access ------------------------------------------------------
    /// Push value of variable. Operand = constant pool index of symbol name.
    VarRef(u16),
    /// Set variable to TOS (pops). Operand = constant pool index of symbol name.
    VarSet(u16),
    /// Bind variable in new dynamic scope frame. Operand = constant pool index.
    VarBind(u16),
    /// Unbind the N most recent dynamic bindings.
    Unbind(u16),

    // -- Function calls -------------------------------------------------------
    /// Call function on stack with N args.
    /// Stack: [func arg1 arg2 ... argN] -> [result]
    Call(u16),
    /// Like Call but also passes the function through apply semantics.
    /// Last arg is spread as a list.
    Apply(u16),

    // -- Control flow ---------------------------------------------------------
    /// Unconditional jump to absolute instruction index.
    Goto(u32),
    /// Jump if TOS is nil (pops TOS).
    GotoIfNil(u32),
    /// Jump if TOS is not nil (pops TOS).
    GotoIfNotNil(u32),
    /// Jump if TOS is nil (preserves TOS), else pop.
    GotoIfNilElsePop(u32),
    /// Jump if TOS is not nil (preserves TOS), else pop.
    GotoIfNotNilElsePop(u32),
    /// Pop a hash-table jump table and a dispatch value, then branch on match.
    ///
    /// GNU-decoded bytecode stores jump targets as original byte offsets inside
    /// the hash-table constant; NeoVM-compiled bytecode may use direct
    /// instruction indices. The VM resolves GNU offsets through the decoded
    /// function's byte-offset map at runtime.
    Switch,
    /// Return TOS as function result.
    Return,

    // -- Arithmetic -----------------------------------------------------------
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Add1,
    Sub1,
    Negate,

    // -- Comparison -----------------------------------------------------------
    /// Numeric =
    Eqlsign,
    /// >
    Gtr,
    /// <
    Lss,
    /// <=
    Leq,
    /// >=
    Geq,
    Max,
    Min,

    // -- List operations ------------------------------------------------------
    Car,
    Cdr,
    Cons,
    /// Create list from N stack elements.
    List(u16),
    Length,
    Nth,
    Nthcdr,
    Setcar,
    Setcdr,
    CarSafe,
    CdrSafe,
    Elt,
    Nconc,
    Nreverse,
    Member,
    Memq,
    Assq,

    // -- Type predicates ------------------------------------------------------
    Symbolp,
    Consp,
    Stringp,
    Listp,
    Integerp,
    Numberp,
    Null,
    Not,
    Eq,
    Equal,

    // -- String operations ----------------------------------------------------
    /// Concat N strings from the stack.
    Concat(u16),
    Substring,
    StringEqual,
    StringLessp,

    // -- Vector operations ----------------------------------------------------
    Aref,
    Aset,

    // -- Symbol operations ----------------------------------------------------
    SymbolValue,
    SymbolFunction,
    Set,
    Fset,
    Get,
    Put,

    // -- Error handling -------------------------------------------------------
    /// Push a condition-case handler.
    /// Operand = jump target (instruction index) for handler body.
    PushConditionCase(u32),
    /// GNU bytecode `pushconditioncase`: pops handler pattern object and records
    /// handler target for this frame.
    PushConditionCaseRaw(u32),
    /// GNU bytecode `pushcatch`: pops catch tag and records handler target.
    PushCatch(u32),
    /// Pop the most recent condition-case handler.
    PopHandler,
    /// GNU-style unwind-protect: pop cleanup handler function from TOS.
    /// Used when decoding GNU bytecodes where byte-unwind-protect (142)
    /// pops a cleanup function rather than jumping to a code offset.
    UnwindProtectPop,
    /// Signal an error (throw).
    Throw,

    // -- Bytecode unwind/specpdl ops ----------------------------------------
    /// GNU `byte-save-current-buffer`.
    SaveCurrentBuffer,
    /// GNU `byte-save-excursion`.
    SaveExcursion,
    /// GNU `byte-save-restriction`.
    SaveRestriction,
    /// GNU `Bsave_window_excursion` (opcode 139). Obsolete since Emacs 24
    /// but still present in .elc files. Pop body, Fprogn it inside
    /// save-window-excursion, push result.
    SaveWindowExcursion,

    // -- Closure support ------------------------------------------------------
    /// Create a closure from a bytecode function object at constant pool index,
    /// capturing the current lexical environment.
    MakeClosure(u16),

    // -- Misc -----------------------------------------------------------------
    /// Call a named builtin (constant pool index for name) with N args.
    /// This is the escape hatch for builtins not covered by dedicated opcodes.
    CallBuiltin(u16, u8),
    /// Call a named builtin by direct symbol reference, with N args.
    ///
    /// Mirrors GNU's inline dispatch of opcodes 0140-0177 (Bpoint,
    /// Bgoto_char, Bcurrent_buffer, ...) in `src/bytecode.c`, which
    /// call the underlying C functions directly without any constants
    /// pool lookup. Using this variant for those opcodes keeps the
    /// decoded constants vector identical in size to the source
    /// bytecode's constants — otherwise a too-short pool upstream
    /// (e.g. a cl-generic dispatch compiled with a shared bytecode
    /// template) silently shifts Op::Constant(N) references into the
    /// appended builtin symbols.
    CallBuiltinSym(crate::emacs_core::intern::SymId, u8),
    /// A `Constant` whose pool index was proven out of range at decode time.
    ///
    /// `seal_ops` rewrites such instructions so the hot `Constant` arm can
    /// read the pool unchecked; executing this op reproduces the exact
    /// runtime error the checked arm used to raise. GNU reads its constant
    /// vector unchecked here, so this shape only exists in malformed input.
    TrapOutOfRangeConstant(u16),
}

impl Op {
    /// Human-readable disassembly of this instruction.
    pub fn disasm(&self, constants: &[super::super::value::Value]) -> String {
        match self {
            Op::Constant(idx) => {
                let val = constants
                    .get(*idx as usize)
                    .map(|v| format!("{}", v))
                    .unwrap_or_else(|| "???".to_string());
                format!("constant {} ; {}", idx, val)
            }
            Op::TrapOutOfRangeConstant(idx) => {
                format!("constant {} ; <out of range>", idx)
            }
            Op::Nil => "nil".to_string(),
            Op::True => "true".to_string(),
            Op::Pop => "pop".to_string(),
            Op::Dup => "dup".to_string(),
            Op::StackRef(n) => format!("stack-ref {}", n),
            Op::StackSet(n) => format!("stack-set {}", n),
            Op::DiscardN(n) => format!("discard-n {}", n),
            Op::VarRef(idx) => {
                let name = const_name(constants, *idx);
                format!("varref {} ; {}", idx, name)
            }
            Op::VarSet(idx) => {
                let name = const_name(constants, *idx);
                format!("varset {} ; {}", idx, name)
            }
            Op::VarBind(idx) => {
                let name = const_name(constants, *idx);
                format!("varbind {} ; {}", idx, name)
            }
            Op::Unbind(n) => format!("unbind {}", n),
            Op::Call(n) => format!("call {}", n),
            Op::Apply(n) => format!("apply {}", n),
            Op::Goto(addr) => format!("goto {}", addr),
            Op::GotoIfNil(addr) => format!("goto-if-nil {}", addr),
            Op::GotoIfNotNil(addr) => format!("goto-if-not-nil {}", addr),
            Op::GotoIfNilElsePop(addr) => format!("goto-if-nil-else-pop {}", addr),
            Op::GotoIfNotNilElsePop(addr) => format!("goto-if-not-nil-else-pop {}", addr),
            Op::Switch => "switch".to_string(),
            Op::Return => "return".to_string(),
            Op::Add => "add".to_string(),
            Op::Sub => "sub".to_string(),
            Op::Mul => "mul".to_string(),
            Op::Div => "div".to_string(),
            Op::Rem => "rem".to_string(),
            Op::Add1 => "add1".to_string(),
            Op::Sub1 => "sub1".to_string(),
            Op::Negate => "negate".to_string(),
            Op::Eqlsign => "eqlsign".to_string(),
            Op::Gtr => "gtr".to_string(),
            Op::Lss => "lss".to_string(),
            Op::Leq => "leq".to_string(),
            Op::Geq => "geq".to_string(),
            Op::Max => "max".to_string(),
            Op::Min => "min".to_string(),
            Op::Car => "car".to_string(),
            Op::Cdr => "cdr".to_string(),
            Op::Cons => "cons".to_string(),
            Op::List(n) => format!("list {}", n),
            Op::Length => "length".to_string(),
            Op::Nth => "nth".to_string(),
            Op::Nthcdr => "nthcdr".to_string(),
            Op::Setcar => "setcar".to_string(),
            Op::Setcdr => "setcdr".to_string(),
            Op::CarSafe => "car-safe".to_string(),
            Op::CdrSafe => "cdr-safe".to_string(),
            Op::Elt => "elt".to_string(),
            Op::Nconc => "nconc".to_string(),
            Op::Nreverse => "nreverse".to_string(),
            Op::Member => "member".to_string(),
            Op::Memq => "memq".to_string(),
            Op::Assq => "assq".to_string(),
            Op::Symbolp => "symbolp".to_string(),
            Op::Consp => "consp".to_string(),
            Op::Stringp => "stringp".to_string(),
            Op::Listp => "listp".to_string(),
            Op::Integerp => "integerp".to_string(),
            Op::Numberp => "numberp".to_string(),
            Op::Null => "null".to_string(),
            Op::Not => "not".to_string(),
            Op::Eq => "eq".to_string(),
            Op::Equal => "equal".to_string(),
            Op::Concat(n) => format!("concat {}", n),
            Op::Substring => "substring".to_string(),
            Op::StringEqual => "string-equal".to_string(),
            Op::StringLessp => "string-lessp".to_string(),
            Op::Aref => "aref".to_string(),
            Op::Aset => "aset".to_string(),
            Op::SymbolValue => "symbol-value".to_string(),
            Op::SymbolFunction => "symbol-function".to_string(),
            Op::Set => "set".to_string(),
            Op::Fset => "fset".to_string(),
            Op::Get => "get".to_string(),
            Op::Put => "put".to_string(),
            Op::PushConditionCase(addr) => format!("push-condition-case {}", addr),
            Op::PushConditionCaseRaw(addr) => format!("push-condition-case-raw {}", addr),
            Op::PushCatch(addr) => format!("push-catch {}", addr),
            Op::PopHandler => "pop-handler".to_string(),
            Op::UnwindProtectPop => "unwind-protect-pop".to_string(),
            Op::Throw => "throw".to_string(),
            Op::SaveCurrentBuffer => "save-current-buffer".to_string(),
            Op::SaveExcursion => "save-excursion".to_string(),
            Op::SaveRestriction => "save-restriction".to_string(),
            Op::SaveWindowExcursion => "save-window-excursion".to_string(),
            Op::MakeClosure(idx) => format!("make-closure {}", idx),
            Op::CallBuiltin(idx, n) => {
                let name = const_name(constants, *idx);
                format!("call-builtin {} {} ; {}", idx, n, name)
            }
            Op::CallBuiltinSym(sym, n) => {
                let name = crate::emacs_core::intern::resolve_sym(*sym);
                format!("call-builtin-sym {} {}", name, n)
            }
        }
    }
}

fn const_name(constants: &[super::super::value::Value], idx: u16) -> String {
    constants
        .get(idx as usize)
        .and_then(|v| v.as_symbol_name().or_else(|| v.as_utf8_str()))
        .unwrap_or("???")
        .to_string()
}
#[cfg(test)]
#[path = "tests/opcode.rs"]
mod tests;
