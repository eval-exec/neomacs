//! Bytecode opcodes for the neovm bytecode compiler and VM.
//!
//! Uses a high-level `Op` enum where each variant carries its operands.
//! This is easier to work with during compilation. A future optimization
//! pass can serialize to a compact byte stream.

crate::emacs_core::subr::world_registry!(Op);

pub use crate::tagged::bytecode::opcode::Op;

// Branch classification serves only JIT compilation and its tests.
#[cfg(any(test, feature = "jit"))]
mod branch_targets;
#[cfg(any(test, feature = "jit"))]
pub(crate) use branch_targets::BranchTargets;

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
        .and_then(|v| v.as_bare_symbol_name().or_else(|| v.as_utf8_str()))
        .unwrap_or("???")
        .to_string()
}
#[cfg(test)]
#[path = "tests/opcode_test.rs"]
mod tests;
