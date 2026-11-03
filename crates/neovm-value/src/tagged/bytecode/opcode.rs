//! Bytecode instruction vocabulary (T1 storage cut).

use serde::{Deserialize, Serialize};

use super::SymId;

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
    /// Run a primitive directly (constant pool index for its name) with N args.
    /// Internal/manual counterpart of `CallBuiltinSym`; it never represents a
    /// genuine symbol call. Bcall decodes as `Call` and honors function cells.
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
    CallBuiltinSym(SymId, u8),
    /// A `Constant` whose pool index was proven out of range at decode time.
    ///
    /// `seal_ops` rewrites such instructions so the hot `Constant` arm can
    /// read the pool unchecked; executing this op reproduces the exact
    /// runtime error the checked arm used to raise. GNU reads its constant
    /// vector unchecked here, so this shape only exists in malformed input.
    TrapOutOfRangeConstant(u16),
}
