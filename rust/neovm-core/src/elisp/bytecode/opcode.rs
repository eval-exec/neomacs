//! Bytecode opcodes for the neovm bytecode compiler and VM.
//!
//! Uses a high-level `Op` enum where each variant carries its operands.
//! This is easier to work with during compilation. A future optimization
//! pass can serialize to a compact byte stream.

/// A single bytecode instruction.
#[derive(Clone, Debug, PartialEq)]
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
    Call(u8),
    /// Like Call but also passes the function through apply semantics.
    /// Last arg is spread as a list.
    Apply(u8),

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
    /// Pop the most recent condition-case handler.
    PopHandler,
    /// Push an unwind-protect cleanup form marker.
    /// Operand = start of cleanup code.
    UnwindProtect(u32),
    /// Signal an error (throw).
    Throw,

    // -- Closure support ------------------------------------------------------
    /// Create a closure from a bytecode function object at constant pool index,
    /// capturing the current lexical environment.
    MakeClosure(u16),

    // -- Misc -----------------------------------------------------------------
    /// Call a named builtin (constant pool index for name) with N args.
    /// This is the escape hatch for builtins not covered by dedicated opcodes.
    CallBuiltin(u16, u8),
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
            Op::Nil => "nil".to_string(),
            Op::True => "true".to_string(),
            Op::Pop => "pop".to_string(),
            Op::Dup => "dup".to_string(),
            Op::StackRef(n) => format!("stack-ref {}", n),
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
            Op::PopHandler => "pop-handler".to_string(),
            Op::UnwindProtect(addr) => format!("unwind-protect {}", addr),
            Op::Throw => "throw".to_string(),
            Op::MakeClosure(idx) => format!("make-closure {}", idx),
            Op::CallBuiltin(idx, n) => {
                let name = const_name(constants, *idx);
                format!("call-builtin {} {} ; {}", idx, n, name)
            }
        }
    }
}

fn const_name(constants: &[super::super::value::Value], idx: u16) -> String {
    constants
        .get(idx as usize)
        .and_then(|v| v.as_symbol_name().or_else(|| v.as_str()))
        .unwrap_or("???")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::Value;

    #[test]
    fn op_disasm_constant() {
        let constants = vec![Value::Int(42)];
        assert_eq!(Op::Constant(0).disasm(&constants), "constant 0 ; 42");
    }

    #[test]
    fn op_disasm_varref() {
        let constants = vec![Value::symbol("x")];
        assert_eq!(Op::VarRef(0).disasm(&constants), "varref 0 ; x");
    }

    #[test]
    fn op_disasm_simple() {
        let c: Vec<Value> = vec![];
        assert_eq!(Op::Add.disasm(&c), "add");
        assert_eq!(Op::Return.disasm(&c), "return");
        assert_eq!(Op::Goto(10).disasm(&c), "goto 10");
    }
}
