//! Bytecode opcodes for the neovm bytecode compiler and VM.
//!
//! Uses a high-level `Op` enum where each variant carries its operands.
//! This is easier to work with during compilation. A future optimization
//! pass can serialize to a compact byte stream.

crate::emacs_core::subr::world_registry!(Op);

pub use neovm_value::tagged::bytecode::opcode::Op;

use crate::emacs_core::bytecode::ByteCodeFunctionExt as _;

// Branch classification serves only JIT compilation and its tests.
#[cfg(any(test, feature = "jit"))]
mod branch_targets;
#[cfg(any(test, feature = "jit"))]
pub(crate) use branch_targets::{BranchTargets, OpBranchTargetsExt};

/// Upper disassembly on the lowered [`Op`] (human-readable diagnostics only).
pub(crate) trait OpDisasmExt {
    fn disasm(&self, constants: &[neovm_value::value::Value]) -> String;

    /// Like [`Self::disasm`], with the following instruction available for
    /// the context-sensitive arms GNU `disass.el` prints on the constant
    /// line (the jump table of a following `byte-switch`).
    fn disasm_in(&self, constants: &[neovm_value::value::Value], next: Option<&Op>) -> String;
}

/// GNU `prin1` of a constant with `print-escape-newlines` bound (disass.el:290).
pub(crate) fn disasm_value_text(value: &neovm_value::value::Value) -> String {
    crate::emacs_core::print::print_value_with_options(value, disasm_print_options())
}

/// GNU `disassemble-recursive-indent` (disass.el:51).
const DISASSEMBLE_RECURSIVE_INDENT: usize = 3;

/// GNU binds `print-escape-newlines` to t around the constant `prin1`
/// (disass.el:290); everything else keeps `prin1`'s defaults, so unlike
/// `print_value` a disassembly does not force `print-escape-nonascii`.
fn disasm_print_options() -> crate::emacs_core::print::PrintOptions {
    let mut options = crate::emacs_core::print::PrintOptions::default();
    options.print_escape_newlines = true;
    options
}

/// Indent a nested disassembly by one GNU recursive indent per level.
fn indent_disassembly(text: &str) -> String {
    let pad = " ".repeat(DISASSEMBLE_RECURSIVE_INDENT);
    text.lines()
        .map(|line| format!("{pad}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `%s` of a hash-table key in GNU's jump-table line. GNU switch tables
/// key on symbols; the remaining Lisp scalar cases are spelled the way
/// `princ` would. Structural/pointer keys cannot arise from `byte-switch`.
fn hash_key_text(key: &neovm_value::value::HashKey) -> String {
    use neovm_value::value::HashKey;
    match key {
        HashKey::Nil => "nil".to_string(),
        HashKey::True => "t".to_string(),
        HashKey::Int(value) => value.to_string(),
        HashKey::Char(value) => value.to_string(),
        HashKey::Symbol(id) => neovm_value::intern::resolve_sym(*id).to_string(),
        HashKey::Keyword(id) => format!(":{}", neovm_value::intern::resolve_sym(*id)),
        _ => "<key>".to_string(),
    }
}

/// GNU `hash-table-test` as the symbol printed in `<jump-table-TEST ...>`.
fn hash_table_test_text(table: &neovm_value::value::LispHashTable) -> String {
    if let Some(id) = table.test_name {
        return neovm_value::intern::resolve_sym(id).to_string();
    }
    match table.test {
        neovm_value::value::HashTableTest::Eq => "eq".to_string(),
        neovm_value::value::HashTableTest::Eql => "eql".to_string(),
        neovm_value::value::HashTableTest::Equal => "equal".to_string(),
    }
}

/// GNU jump-table arm (disass.el:249-258): the constant before `byte-switch`
/// prints the switch table it feeds.
fn jump_table_text(table: &neovm_value::value::LispHashTable) -> String {
    let options = disasm_print_options();
    let mut out = format!("<jump-table-{} (", hash_table_test_text(table));
    let mut first = true;
    for (key, target) in table.data.iter() {
        if !first {
            out.push(' ');
        }
        first = false;
        out.push_str(&hash_key_text(key));
        out.push(' ');
        out.push_str(&crate::emacs_core::print::print_value_with_options(
            target, options,
        ));
    }
    out.push_str(")>");
    out
}

/// GNU disass.el:260-284 arms for a `byte-constant` whose value is compiled
/// code: `<byte-code-function>` / `<compiled macro>` / `<byte code>` /
/// `(<byte code>...)`, each followed by the recursive disassembly.
fn disasm_constant(
    constants: &[neovm_value::value::Value],
    idx: usize,
    next: Option<&Op>,
) -> String {
    use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt as _;
    use neovm_value::value::Value;

    let Some(value) = constants.get(idx).copied() else {
        return "???".to_string();
    };
    // disass.el:249: a following byte-switch prints the jump table it uses.
    if matches!(next, Some(Op::Switch))
        && let Some(table) = value.as_hash_table()
    {
        return jump_table_text(table);
    }
    if value.is_bytecode()
        && let Some(function) = value.get_bytecode_data()
    {
        return format!(
            "<byte-code-function>\n{}",
            indent_disassembly(&function.disassemble())
        );
    }
    if value.is_cons() {
        let head = value.cons_car();
        let tail = value.cons_cdr();
        if head.as_bare_symbol_name() == Some("macro")
            && tail.is_bytecode()
            && let Some(function) = tail.get_bytecode_data()
        {
            return format!(
                "<compiled macro>\n{}",
                indent_disassembly(&function.disassemble())
            );
        }
        if head.as_bare_symbol_name() == Some("byte-code") {
            return match byte_code_list_function(value) {
                Some(function) => format!(
                    "<byte code>\n{}",
                    indent_disassembly(&function.disassemble())
                ),
                None => "<byte code>".to_string(),
            };
        }
        // disass.el:277: a list whose first element is a byte-code object.
        if head.is_cons() && head.cons_car().as_bare_symbol_name() == Some("byte-code") {
            let mut out = String::from("(<byte code>...)\n");
            let mut items = Some(value);
            while let Some(cell) = items {
                if !cell.is_cons() {
                    break;
                }
                let item = cell.cons_car();
                if let Some(function) = byte_code_list_function(item) {
                    out.push_str(&indent_disassembly(&function.disassemble()));
                    out.push('\n');
                }
                items = Some(cell.cons_cdr()).filter(|rest| rest.is_cons());
            }
            return out.trim_end().to_string();
        }
    }
    disasm_value_text(&value)
}

/// Build the byte-code function a raw `(byte-code BYTES CONSTS DEPTH)` list
/// carries (the only shape GNU's compiler emits; disass.el:271-276).
fn byte_code_list_function(
    value: neovm_value::value::Value,
) -> Option<crate::emacs_core::bytecode::ByteCodeFunction> {
    let mut items = Vec::new();
    let mut cell = value;
    while cell.is_cons() && items.len() < 4 {
        items.push(cell.cons_car());
        cell = cell.cons_cdr();
    }
    let [_, bytes, constants, depth, ..] = items.as_slice() else {
        return None;
    };
    crate::emacs_core::builtins::byte_code_for_immediate_call(bytes, constants, depth).ok()
}

impl OpDisasmExt for Op {
    /// Human-readable disassembly of this instruction.
    fn disasm(&self, constants: &[neovm_value::value::Value]) -> String {
        self.disasm_in(constants, None)
    }

    fn disasm_in(&self, constants: &[neovm_value::value::Value], next: Option<&Op>) -> String {
        match self {
            Op::Constant(idx) => {
                format!(
                    "constant {} ; {}",
                    idx,
                    disasm_constant(constants, *idx as usize, next)
                )
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
                let name = neovm_value::intern::resolve_sym(*sym);
                format!("call-builtin-sym {} {}", name, n)
            }
        }
    }
}

fn const_name(constants: &[neovm_value::value::Value], idx: u16) -> String {
    constants
        .get(idx as usize)
        .and_then(|v| v.as_bare_symbol_name().or_else(|| v.as_utf8_str()))
        .unwrap_or("???")
        .to_string()
}
#[cfg(test)]
#[path = "tests/opcode_test.rs"]
mod tests;
