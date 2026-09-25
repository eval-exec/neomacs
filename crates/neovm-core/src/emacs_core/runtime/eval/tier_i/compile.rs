//! The Tier-I compiler (T4's analyzer, fallback-B shape): a lambda body's
//! conses, compiled into a tree of [`Node`]s that mirrors them.
//!
//! The compiler never decides anything the executor does not re-check at
//! run time: every node records the object it was compiled from and is used
//! only while the live code still holds that object at that position; a
//! special-form node is used only while its head is still that special form
//! (function epoch); lexical/dynamic binding is decided at run time exactly
//! as the tree walker decides it.  What the compiler adds is the structure:
//! which subform comes next, and which binders a variable reference may
//! resolve to (its candidate slots, innermost first).

// The executor (fallback B) reads the fields the analyzer's report does not.
#![allow(dead_code)]

use super::super::*;
use std::fmt;

cached_symbol_id!(optional_arg_symbol, "&optional");
cached_symbol_id!(rest_arg_symbol, "&rest");

/// The most cons forms compiled in one body; a larger body is refused.
const MAX_FORMS: u32 = 50_000;
/// The deepest nesting compiled; a deeper (or cyclic) body is refused.
const MAX_NESTING: u32 = 2_000;
/// The longest list walked for one special form or call.
const MAX_LIST: usize = 10_000;

/// A slot in an activation's slot array.
pub(super) type Slot = u16;

/// Candidate slots of one variable reference: every binder of its symbol in
/// scope, innermost first.
pub(super) type Candidates = SmallVec<[Slot; 2]>;

/// One compiled form.
pub(super) enum Node {
    /// A self-evaluating atom (not a symbol, not a cons): `eval_sub` returns
    /// it unchanged.
    Const(Value),
    /// A symbol reference.
    Var(VarNode),
    /// A cons form whose head is a plain symbol.
    Form(Box<FormNode>),
    /// A form left to `eval_sub` (a symbol with position, a non-symbol head,
    /// a macro call, a malformed special form).
    Eval(Value),
}

impl Node {
    /// The object this node was compiled from.
    #[inline(always)]
    pub(super) fn form(&self) -> Value {
        match self {
            Node::Const(value) | Node::Eval(value) => *value,
            Node::Var(var) => var.symbol,
            Node::Form(form) => form.form,
        }
    }
}

/// A symbol reference.
pub(super) struct VarNode {
    pub(super) symbol: Value,
    pub(super) sym: SymId,
    pub(super) slots: Candidates,
}

/// A cons form with a plain-symbol head.
pub(super) struct FormNode {
    /// The form cons.
    pub(super) form: Value,
    /// Its car at compile time (a plain symbol).
    pub(super) head: Value,
    pub(super) head_id: SymId,
    /// Its cdr at compile time.
    pub(super) tail: Value,
    /// The head's class, stamped with the function epoch it was read at.
    pub(super) head_cache: Cell<(u64, FormHead)>,
    pub(super) op: Op,
}

/// What a form node does once its head is confirmed.
pub(super) enum Op {
    /// A call of a subr, byte-code object or interpreted closure; one child
    /// per argument form.
    Call(Seq),
    /// A special form the tree walker's own handler runs (`quote`,
    /// `function`, `defvar`, `defconst`, `interactive`).
    Special(SpecialFormHandler),
    Progn(Seq),
    And(Seq),
    Or(Seq),
    If {
        cond: Node,
        then: Node,
        otherwise: Seq,
    },
    Cond(Box<[CondClause]>),
    While {
        test: Node,
        body: Seq,
    },
    Setq(Box<[SetqPair]>),
    Let(LetOp),
    LetStar(LetOp),
    Prog1 {
        first: Node,
        rest: Seq,
    },
    Catch {
        tag: Node,
        body: Seq,
    },
    UnwindProtect {
        body: Node,
    },
    ConditionCase {
        body: Node,
    },
    SaveExcursion(Seq),
    SaveRestriction(Seq),
    SaveCurrentBuffer(Seq),
}

impl Op {
    /// The special form this op mirrors, `None` for a call.
    pub(super) fn handler(&self) -> Option<SpecialFormHandler> {
        Some(match self {
            Op::Call(_) => return None,
            Op::Special(handler) => *handler,
            Op::Progn(_) => SpecialFormHandler::Progn,
            Op::And(_) => SpecialFormHandler::And,
            Op::Or(_) => SpecialFormHandler::Or,
            Op::If { .. } => SpecialFormHandler::If,
            Op::Cond(_) => SpecialFormHandler::Cond,
            Op::While { .. } => SpecialFormHandler::While,
            Op::Setq(_) => SpecialFormHandler::Setq,
            Op::Let(_) => SpecialFormHandler::Let,
            Op::LetStar(_) => SpecialFormHandler::LetStar,
            Op::Prog1 { .. } => SpecialFormHandler::Prog1,
            Op::Catch { .. } => SpecialFormHandler::Catch,
            Op::UnwindProtect { .. } => SpecialFormHandler::UnwindProtect,
            Op::ConditionCase { .. } => SpecialFormHandler::ConditionCase,
            Op::SaveExcursion(_) => SpecialFormHandler::SaveExcursion,
            Op::SaveRestriction(_) => SpecialFormHandler::SaveRestriction,
            Op::SaveCurrentBuffer(_) => SpecialFormHandler::SaveCurrentBuffer,
        })
    }
}

/// The nodes of a list of forms, by position.
pub(super) struct Seq(Box<[Node]>);

impl Seq {
    #[inline(always)]
    pub(super) fn get(&self, index: usize) -> Option<&Node> {
        self.0.get(index)
    }

    fn empty() -> Self {
        Seq(Box::new([]))
    }
}

/// One `cond` clause `(TEST . BODY)`.
pub(super) struct CondClause {
    /// The clause cons.
    pub(super) clause: Value,
    pub(super) test: Node,
    pub(super) body: Seq,
}

/// One `setq` pair.
pub(super) struct SetqPair {
    /// The symbol as written (a plain symbol).
    pub(super) symbol: Value,
    pub(super) value: Node,
    pub(super) slots: Candidates,
}

/// A `let` or `let*`.
pub(super) struct LetOp {
    /// The varlist at compile time.
    pub(super) varlist: Value,
    pub(super) bindings: Box<[LetBinding]>,
    pub(super) body: Seq,
}

/// One binding of a `let` or `let*`.
pub(super) struct LetBinding {
    /// The varlist element: a plain symbol, or the binding cons.
    pub(super) element: Value,
    /// The symbol it binds.
    pub(super) sym: SymId,
    /// The init form's node, when it has one.
    pub(super) init: Option<Node>,
    /// This binder's slot.
    pub(super) slot: Slot,
}

/// How much of a body the compiler covers (the T4 coverage census).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompileSummary {
    /// Cons forms in the body.
    pub(crate) forms: u32,
    /// Calls (subr, byte-code or closure heads at compile time).
    pub(crate) calls: u32,
    /// Special forms the executor mirrors.
    pub(crate) mirrored: u32,
    /// Special forms run by the tree walker's handler.
    pub(crate) handled: u32,
    /// Forms left to `eval_sub` whole (macros, odd heads, malformed forms).
    pub(crate) evals: u32,
    /// Variable references.
    pub(crate) vars: u32,
    /// Variable references with a candidate slot.
    pub(crate) slotted: u32,
    /// Binders (formals, `let` and `let*` bindings).
    pub(crate) binders: u32,
}

impl CompileSummary {
    pub(crate) fn add(&mut self, other: &CompileSummary) {
        self.forms += other.forms;
        self.calls += other.calls;
        self.mirrored += other.mirrored;
        self.handled += other.handled;
        self.evals += other.evals;
        self.vars += other.vars;
        self.slotted += other.slotted;
        self.binders += other.binders;
    }
}

impl fmt::Display for CompileSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "forms={} calls={} mirrored={} handled={} evals={} vars={} slotted={} binders={}",
            self.forms,
            self.calls,
            self.mirrored,
            self.handled,
            self.evals,
            self.vars,
            self.slotted,
            self.binders
        )
    }
}

/// A compiled lambda body.
pub(crate) struct TierCode {
    /// The arglist and body it was compiled from.
    pub(super) arglist: Value,
    pub(super) body: Value,
    /// The formals in arglist order; formal I owns slot I.
    pub(super) formals: Box<[SymId]>,
    /// Slots an activation needs.
    pub(super) nslots: usize,
    /// The body forms.
    pub(super) seq: Seq,
    /// Every heap value the nodes hold.
    roots: Box<[Value]>,
    summary: CompileSummary,
}

impl TierCode {
    pub(super) fn roots(&self) -> &[Value] {
        &self.roots
    }

    pub(crate) fn summary(&self) -> &CompileSummary {
        &self.summary
    }
}

/// Compile a lambda with ARGLIST and BODY, or `None` when it cannot be (a
/// malformed arglist, a body that is not a list, too large or too deep).
pub(super) fn compile_body(obarray: &Obarray, arglist: Value, body: Value) -> Option<TierCode> {
    let mut compiler = Compiler {
        obarray,
        scope: Vec::new(),
        nslots: 0,
        roots: Vec::new(),
        summary: CompileSummary::default(),
        depth: 0,
        refused: false,
    };
    compiler.root(arglist);
    compiler.root(body);
    let formals = compiler.formals(arglist)?;
    for sym in &formals {
        compiler.bind(*sym)?;
    }
    let seq = compiler.seq(body);
    if compiler.refused {
        return None;
    }
    Some(TierCode {
        arglist,
        body,
        formals: formals.into_boxed_slice(),
        nslots: compiler.nslots,
        seq,
        roots: compiler.roots.into_boxed_slice(),
        summary: compiler.summary,
    })
}

struct Compiler<'a> {
    obarray: &'a Obarray,
    /// Binders in scope, innermost last.
    scope: Vec<(SymId, Slot)>,
    nslots: usize,
    roots: Vec<Value>,
    summary: CompileSummary,
    depth: u32,
    refused: bool,
}

impl Compiler<'_> {
    fn root(&mut self, value: Value) {
        if value.is_heap_object() {
            self.roots.push(value);
        }
    }

    /// The formal symbols of ARGLIST in order, as `walk_lambda_formals` binds
    /// them; `None` for an arglist it would reject.
    fn formals(&self, arglist: Value) -> Option<Vec<SymId>> {
        let mut out = Vec::new();
        let mut cursor = arglist;
        let mut n = 0;
        while cursor.is_cons() {
            let item = cursor.cons_car();
            if item.is_symbol_with_pos() {
                return None;
            }
            let id = item.as_symbol_id()?;
            if id != optional_arg_symbol() && id != rest_arg_symbol() {
                out.push(id);
            }
            cursor = cursor.cons_cdr();
            n += 1;
            if n > MAX_LIST {
                return None;
            }
        }
        cursor.is_nil().then_some(out)
    }

    /// A new binder of SYM in scope; its slot.
    fn bind(&mut self, sym: SymId) -> Option<Slot> {
        let slot = Slot::try_from(self.nslots).ok()?;
        self.nslots += 1;
        self.scope.push((sym, slot));
        self.summary.binders += 1;
        Some(slot)
    }

    /// Every binder of SYM in scope, innermost first.  All of them: the
    /// executor concludes "no lexical cell" when none is filled.
    fn candidates(&self, sym: SymId) -> Candidates {
        self.scope
            .iter()
            .rev()
            .filter(|(bound, _)| *bound == sym)
            .map(|(_, slot)| *slot)
            .collect()
    }

    /// The nodes of the list LIST, by position (its proper prefix).
    fn seq(&mut self, list: Value) -> Seq {
        let mut nodes = Vec::new();
        let mut cursor = list;
        while cursor.is_cons() {
            if nodes.len() >= MAX_LIST {
                self.refused = true;
                break;
            }
            let node = self.node(cursor.cons_car());
            nodes.push(node);
            cursor = cursor.cons_cdr();
        }
        Seq(nodes.into_boxed_slice())
    }

    fn node(&mut self, form: Value) -> Node {
        if self.refused {
            return Node::Eval(form);
        }
        self.root(form);
        if form.is_symbol_with_pos() {
            return Node::Eval(form);
        }
        if let Some(sym) = form.as_symbol_id() {
            self.summary.vars += 1;
            let slots = self.candidates(sym);
            if !slots.is_empty() {
                self.summary.slotted += 1;
            }
            return Node::Var(VarNode {
                symbol: form,
                sym,
                slots,
            });
        }
        if !form.is_cons() {
            return Node::Const(form);
        }
        self.summary.forms += 1;
        if self.summary.forms > MAX_FORMS || self.depth > MAX_NESTING {
            self.refused = true;
            return Node::Eval(form);
        }
        self.depth += 1;
        let node = self.cons_form(form);
        self.depth -= 1;
        node
    }

    /// A cons form: a plain-symbol head classified against its current
    /// function cell.
    fn cons_form(&mut self, form: Value) -> Node {
        let head = form.cons_car();
        let tail = form.cons_cdr();
        let Some(head_id) = head.as_symbol_id().filter(|_| !head.is_symbol_with_pos()) else {
            self.summary.evals += 1;
            return Node::Eval(form);
        };
        let class = FormHead::classify(head_id, self.obarray.symbol_function_id(head_id));
        if class.literal_head {
            self.summary.evals += 1;
            return Node::Eval(form);
        }
        let op = match class.class {
            HeadClass::SpecialForm(handler) => match self.special(handler, tail) {
                Some(op) => op,
                None => {
                    self.summary.evals += 1;
                    return Node::Eval(form);
                }
            },
            HeadClass::Subr { .. } | HeadClass::ByteCode | HeadClass::Lambda => {
                self.summary.calls += 1;
                Op::Call(self.seq(tail))
            }
            HeadClass::Slow => {
                if class
                    .func
                    .is_some_and(|func| cell_is_macro(self.obarray, func))
                {
                    self.summary.evals += 1;
                    return Node::Eval(form);
                }
                // A function the tree walker resolves the long way (an alias,
                // an autoloaded function, `throw`, no definition yet): the
                // node dispatches it through the tree walker while it stays
                // that way, and takes the fast path once it is defined.
                self.summary.calls += 1;
                Op::Call(self.seq(tail))
            }
        };
        self.root(tail);
        Node::Form(Box::new(FormNode {
            form,
            head,
            head_id,
            tail,
            head_cache: Cell::new((EMPTY_HEAD_EPOCH, class)),
            op,
        }))
    }

    /// The op for special form HANDLER with arguments TAIL, or `None` to
    /// leave the form to `eval_sub` (a shape the handler would reject).
    fn special(&mut self, handler: SpecialFormHandler, tail: Value) -> Option<Op> {
        use SpecialFormHandler as H;
        let op = match handler {
            H::Quote | H::Function | H::Defvar | H::Defconst | H::Interactive => {
                self.summary.handled += 1;
                return Some(Op::Special(handler));
            }
            H::Progn => Op::Progn(self.proper_seq(tail)?),
            H::And => Op::And(self.proper_seq(tail)?),
            H::Or => Op::Or(self.proper_seq(tail)?),
            H::SaveExcursion => Op::SaveExcursion(self.proper_seq(tail)?),
            H::SaveRestriction => Op::SaveRestriction(self.proper_seq(tail)?),
            H::SaveCurrentBuffer => Op::SaveCurrentBuffer(self.proper_seq(tail)?),
            H::If => {
                let (cond, rest) = split(tail)?;
                let (then, otherwise) = split(rest)?;
                proper_list(otherwise)?;
                Op::If {
                    cond: self.node(cond),
                    then: self.node(then),
                    otherwise: self.seq(otherwise),
                }
            }
            H::While => {
                let (test, body) = split(tail)?;
                proper_list(body)?;
                Op::While {
                    test: self.node(test),
                    body: self.seq(body),
                }
            }
            H::Prog1 => {
                let (first, rest) = split(tail)?;
                proper_list(rest)?;
                Op::Prog1 {
                    first: self.node(first),
                    rest: self.seq(rest),
                }
            }
            H::Catch => {
                let (tag, body) = split(tail)?;
                proper_list(body)?;
                Op::Catch {
                    tag: self.node(tag),
                    body: self.seq(body),
                }
            }
            H::UnwindProtect => {
                let (body, cleanup) = split(tail)?;
                proper_list(cleanup)?;
                self.root(cleanup);
                Op::UnwindProtect {
                    body: self.node(body),
                }
            }
            H::ConditionCase => {
                let (var, rest) = split(tail)?;
                plain_symbol(var)?;
                let (body, handlers) = split(rest)?;
                proper_list(handlers)?;
                self.root(handlers);
                Op::ConditionCase {
                    body: self.node(body),
                }
            }
            H::Cond => Op::Cond(self.cond(tail)?),
            H::Setq => Op::Setq(self.setq(tail)?),
            H::Let => Op::Let(self.let_op(tail, false)?),
            H::LetStar => Op::LetStar(self.let_op(tail, true)?),
        };
        self.summary.mirrored += 1;
        Some(op)
    }

    fn proper_seq(&mut self, list: Value) -> Option<Seq> {
        proper_list(list)?;
        Some(self.seq(list))
    }

    fn cond(&mut self, tail: Value) -> Option<Box<[CondClause]>> {
        proper_list(tail)?;
        let mut clauses = Vec::new();
        let mut cursor = tail;
        while cursor.is_cons() {
            let clause = cursor.cons_car();
            cursor = cursor.cons_cdr();
            if clause.is_nil() {
                // Skipped at run time; keep the position.
                clauses.push(CondClause {
                    clause,
                    test: Node::Const(Value::NIL),
                    body: Seq::empty(),
                });
                continue;
            }
            let (test, body) = split(clause)?;
            proper_list(body)?;
            self.root(clause);
            clauses.push(CondClause {
                clause,
                test: self.node(test),
                body: self.seq(body),
            });
        }
        Some(clauses.into_boxed_slice())
    }

    fn setq(&mut self, tail: Value) -> Option<Box<[SetqPair]>> {
        proper_list(tail)?;
        let mut pairs = Vec::new();
        let mut cursor = tail;
        while cursor.is_cons() {
            let symbol = cursor.cons_car();
            let (value, rest) = split(cursor.cons_cdr())?;
            let sym = plain_symbol(symbol)?;
            let slots = self.candidates(sym);
            pairs.push(SetqPair {
                symbol,
                value: self.node(value),
                slots,
            });
            cursor = rest;
        }
        Some(pairs.into_boxed_slice())
    }

    /// `let` (STAR false: every init in the outer scope, then every binding)
    /// or `let*` (each init sees the bindings before it).
    fn let_op(&mut self, tail: Value, star: bool) -> Option<LetOp> {
        let (varlist, body) = split(tail)?;
        proper_list(varlist)?;
        proper_list(body)?;
        let scope_mark = self.scope.len();
        let mut parsed = Vec::new();
        let mut cursor = varlist;
        while cursor.is_cons() {
            let element = cursor.cons_car();
            cursor = cursor.cons_cdr();
            let (sym, init) = if let Some(sym) = plain_symbol(element) {
                (sym, None)
            } else {
                let (head, value_tail) = split(element)?;
                let sym = plain_symbol(head)?;
                if value_tail.is_nil() {
                    (sym, None)
                } else {
                    let (init, extra) = split(value_tail)?;
                    if !extra.is_nil() {
                        return None;
                    }
                    (sym, Some(init))
                }
            };
            self.root(element);
            let init = init.map(|init| self.node(init));
            if star {
                let slot = self.bind(sym)?;
                parsed.push(LetBinding {
                    element,
                    sym,
                    init,
                    slot,
                });
            } else {
                parsed.push(LetBinding {
                    element,
                    sym,
                    init,
                    slot: 0,
                });
            }
        }
        if !star {
            for binding in &mut parsed {
                binding.slot = self.bind(binding.sym)?;
            }
        }
        self.root(varlist);
        let body = self.seq(body);
        self.scope.truncate(scope_mark);
        Some(LetOp {
            varlist,
            bindings: parsed.into_boxed_slice(),
            body,
        })
    }
}

/// No function epoch is this value: a fresh node always re-reads its head.
pub(super) const EMPTY_HEAD_EPOCH: u64 = u64::MAX;

/// `(car . cdr)` of a cons, `None` for anything else.
fn split(value: Value) -> Option<(Value, Value)> {
    value
        .is_cons()
        .then(|| (value.cons_car(), value.cons_cdr()))
}

/// `Some` when LIST is a proper list no longer than [`MAX_LIST`].
fn proper_list(list: Value) -> Option<()> {
    let mut cursor = list;
    let mut n = 0;
    while cursor.is_cons() {
        cursor = cursor.cons_cdr();
        n += 1;
        if n > MAX_LIST {
            return None;
        }
    }
    cursor.is_nil().then_some(())
}

/// The id of a plain symbol (never a symbol with position).
fn plain_symbol(value: Value) -> Option<SymId> {
    if value.is_symbol_with_pos() {
        return None;
    }
    value.as_symbol_id()
}

/// Whether a function cell is a macro as `eval_sub` would expand it: a
/// macro object, `(macro . F)`, or an autoload whose type is `macro`.
fn cell_is_macro(obarray: &Obarray, func: Value) -> bool {
    let _ = obarray;
    if func.is_macro() || cons_head_symbol_id(&func) == Some(macro_symbol()) {
        return true;
    }
    if super::super::super::autoload::is_autoload_value(&func) {
        // (autoload FILE DOC INTERACTIVE TYPE)
        let mut cursor = func;
        for _ in 0..4 {
            if !cursor.is_cons() {
                return false;
            }
            cursor = cursor.cons_cdr();
        }
        return cursor.is_cons() && {
            let kind = cursor.cons_car();
            kind.is_symbol_named("macro") || kind.is_t()
        };
    }
    false
}

// ---------------------------------------------------------------------------
// A readable dump of the compiled tree (the analyzer's golden tests and the
// `analyze` report's examples).
// ---------------------------------------------------------------------------

impl TierCode {
    /// The compiled body, one line: `[formal@slot ...] form ...`, where a
    /// variable prints its candidate slots (`x@2.0`), a form left to the tree
    /// walker prints `!FORM`, a call `(call HEAD ...)` and a mirrored special
    /// form its own name with slot-annotated binders.
    pub(crate) fn describe(&self) -> String {
        let mut out = String::from("[");
        for (slot, sym) in self.formals.iter().enumerate() {
            if slot > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{}@{slot}", resolve_sym(*sym)));
        }
        out.push(']');
        describe_seq(&self.seq, &mut out);
        out
    }
}

fn describe_seq(seq: &Seq, out: &mut String) {
    for node in seq.0.iter() {
        out.push(' ');
        describe_node(node, out);
    }
}

fn describe_slots(slots: &Candidates, out: &mut String) {
    if slots.is_empty() {
        return;
    }
    out.push('@');
    let text: Vec<String> = slots.iter().map(u16::to_string).collect();
    out.push_str(&text.join("."));
}

fn describe_node(node: &Node, out: &mut String) {
    match node {
        Node::Const(value) => out.push_str(&super::super::super::print::print_value(value)),
        Node::Eval(value) => {
            out.push('!');
            out.push_str(&super::super::super::print::print_value(value));
        }
        Node::Var(var) => {
            out.push_str(resolve_sym(var.sym));
            describe_slots(&var.slots, out);
        }
        Node::Form(form) => {
            let head = resolve_sym(form.head_id);
            match &form.op {
                Op::Call(args) => {
                    out.push_str(&format!("(call {head}"));
                    describe_seq(args, out);
                }
                Op::Special(_) => {
                    out.push_str(&format!(
                        "(handler {}",
                        super::super::super::print::print_value(&form.form)
                    ));
                }
                Op::Progn(seq)
                | Op::And(seq)
                | Op::Or(seq)
                | Op::SaveExcursion(seq)
                | Op::SaveRestriction(seq)
                | Op::SaveCurrentBuffer(seq) => {
                    out.push_str(&format!("({head}"));
                    describe_seq(seq, out);
                }
                Op::If {
                    cond,
                    then,
                    otherwise,
                } => {
                    out.push_str("(if ");
                    describe_node(cond, out);
                    out.push(' ');
                    describe_node(then, out);
                    describe_seq(otherwise, out);
                }
                Op::Cond(clauses) => {
                    out.push_str("(cond");
                    for clause in clauses.iter() {
                        out.push_str(" (");
                        describe_node(&clause.test, out);
                        describe_seq(&clause.body, out);
                        out.push(')');
                    }
                }
                Op::While { test, body } => {
                    out.push_str("(while ");
                    describe_node(test, out);
                    describe_seq(body, out);
                }
                Op::Setq(pairs) => {
                    out.push_str("(setq");
                    for pair in pairs.iter() {
                        out.push(' ');
                        out.push_str(&super::super::super::print::print_value(&pair.symbol));
                        describe_slots(&pair.slots, out);
                        out.push(' ');
                        describe_node(&pair.value, out);
                    }
                }
                Op::Let(let_op) | Op::LetStar(let_op) => {
                    out.push_str(&format!("({head} ["));
                    for (i, binding) in let_op.bindings.iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        out.push_str(&format!("{}@{}", resolve_sym(binding.sym), binding.slot));
                        if let Some(init) = &binding.init {
                            out.push('=');
                            describe_node(init, out);
                        }
                    }
                    out.push(']');
                    describe_seq(&let_op.body, out);
                }
                Op::Prog1 { first, rest } => {
                    out.push_str("(prog1 ");
                    describe_node(first, out);
                    describe_seq(rest, out);
                }
                Op::Catch { tag, body } => {
                    out.push_str("(catch ");
                    describe_node(tag, out);
                    describe_seq(body, out);
                }
                Op::UnwindProtect { body } => {
                    out.push_str("(unwind-protect ");
                    describe_node(body, out);
                    out.push_str(" ...");
                }
                Op::ConditionCase { body } => {
                    out.push_str("(condition-case ");
                    describe_node(body, out);
                    out.push_str(" ...");
                }
            }
            out.push(')');
        }
    }
}
