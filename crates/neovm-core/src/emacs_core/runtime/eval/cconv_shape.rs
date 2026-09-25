//! The shape of an interpreted closure's source: what
//! `cconv-make-interpreted-closure`'s analysis can see of `(ARGS . BODY)`
//! (P4.1 S0.3).
//!
//! The trusted analysis (`macroexpand-all` + `cconv-fv` over
//! `#'(lambda ARGS nil . BODY)`, lisp/emacs-lisp/cconv.el:950-966) inspects
//! cons structure, symbol identity and atom types, and nothing else:
//! strings only through `stringp`, vectors and other objects not at all.
//! [`ClosureShape`] records exactly that, in pre-order, so two bodies with
//! equal shapes get the same analysis under the same validated facts.
//! Fixnum values are recorded too, conservatively.
//!
//! [`ClosureFacts`] lists the distinct symbols outside quoted data, with
//! whether each can be a form head (the positions `macroexp--expand-all`
//! may macroexpand or hand to a compiler macro).  The walk is a superset of
//! those positions: it treats the car of every list it reaches as a head,
//! except inside `(quote X)` and `(function SYMBOL)`, and for
//! `(function (lambda ARGS . BODY))` it walks BODY (as
//! macroexp.el:487-497 does) with ARGS as variables.  Refusing more heads
//! than GNU expands is safe; refusing fewer would not be.
//!
//! Both walks are iterative and capped, so cyclic or huge bodies are
//! refused rather than walked forever.

use super::*;

/// Most nodes a closure source may have and still be memoized.
pub(crate) const SHAPE_NODE_CAP: usize = 1 << 16;

/// One pre-order token of a closure source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeTok {
    Cons,
    Nil,
    T,
    Symbol(SymId),
    Fixnum(i64),
    String,
    Float,
    /// Any other object, by type only: the analysis never looks inside.
    Veclike(VecLikeType),
    /// A legacy decoded subr value.
    Subr,
}

/// Why a closure source has no shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeRefusal {
    /// More than [`SHAPE_NODE_CAP`] nodes, or cyclic.
    TooLarge,
    /// A symbol with position: `macroexp-preserve-posification` and the
    /// warning paths read positions.
    SymbolWithPos,
    /// An unbound marker or an object the tokenizer cannot classify.
    Opaque,
}

/// The pre-order tokens of `(ARGS . BODY)`: ARGS first, then BODY.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClosureShape {
    pub(crate) toks: Box<[ShapeTok]>,
    /// Deepest car nesting: the analysis recurses on cars and iterates on
    /// cdrs, so this bounds its Lisp call depth.
    pub(crate) car_depth: u32,
    /// The symbol `interactive` occurs anywhere.
    pub(crate) mentions_interactive: bool,
}

cached_symbol_id!(interactive_shape_symbol, "interactive");
cached_symbol_id!(lambda_shape_symbol, "lambda");

fn classify(value: Value) -> Result<ShapeTok, ShapeRefusal> {
    Ok(match value.kind() {
        ValueKind::Nil => ShapeTok::Nil,
        ValueKind::T => ShapeTok::T,
        ValueKind::Symbol(id) => ShapeTok::Symbol(id),
        ValueKind::Fixnum(n) => ShapeTok::Fixnum(n),
        ValueKind::Cons => ShapeTok::Cons,
        ValueKind::String => ShapeTok::String,
        ValueKind::Float => ShapeTok::Float,
        ValueKind::Subr(_) => ShapeTok::Subr,
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => return Err(ShapeRefusal::SymbolWithPos),
        ValueKind::Veclike(kind) => ShapeTok::Veclike(kind),
        ValueKind::Unbound | ValueKind::Unknown => return Err(ShapeRefusal::Opaque),
    })
}

impl ClosureShape {
    /// Tokenize `(ARGS . BODY)`.
    pub(crate) fn of(args: Value, body: Value) -> Result<Self, ShapeRefusal> {
        let mut toks = Vec::new();
        let mut car_depth = 0u32;
        let mut mentions_interactive = false;
        // Pop order is push order reversed: BODY is pushed first so ARGS
        // comes out first, and within a cons the car before the cdr.
        let mut stack: Vec<(Value, u32)> = vec![(body, 0), (args, 0)];
        while let Some((value, depth)) = stack.pop() {
            if toks.len() >= SHAPE_NODE_CAP {
                return Err(ShapeRefusal::TooLarge);
            }
            let tok = classify(value)?;
            toks.push(tok);
            match tok {
                ShapeTok::Cons => {
                    stack.push((value.cons_cdr(), depth));
                    stack.push((value.cons_car(), depth + 1));
                    car_depth = car_depth.max(depth + 1);
                }
                ShapeTok::Symbol(id) if id == interactive_shape_symbol() => {
                    mentions_interactive = true;
                }
                _ => {}
            }
        }
        Ok(Self {
            toks: toks.into_boxed_slice(),
            car_depth,
            mentions_interactive,
        })
    }

    /// Whether the live `(ARGS . BODY)` has exactly these tokens.  Reads at
    /// most `toks.len()` nodes, so a cyclic or grown body just mismatches.
    pub(crate) fn matches(&self, args: Value, body: Value) -> bool {
        let mut next = 0usize;
        let mut stack: SmallVec<[Value; 32]> = SmallVec::new();
        stack.push(body);
        stack.push(args);
        while let Some(value) = stack.pop() {
            let Some(&expected) = self.toks.get(next) else {
                return false;
            };
            next += 1;
            let Ok(tok) = classify(value) else {
                return false;
            };
            if tok != expected {
                return false;
            }
            if tok == ShapeTok::Cons {
                stack.push(value.cons_cdr());
                stack.push(value.cons_car());
            }
        }
        next == self.toks.len()
    }
}

/// A distinct symbol of a closure source, outside quoted data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SymRole {
    pub(crate) id: SymId,
    /// It is the car of a list the analysis may treat as a form, so a
    /// macro or compiler macro there would change the expansion.
    pub(crate) head: bool,
}

/// Why a closure source's facts cannot be memoized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FactsRefusal {
    /// More than [`SHAPE_NODE_CAP`] nodes walked.
    TooLarge,
    /// A symbol whose name starts with `_` is used (outside a binding
    /// position): a used `_` variable makes `cconv--analyze-use` consult
    /// `byte-compile-warning-enabled-p` (cconv.el:657-661), whose answer is
    /// not part of the memo's key.
    UnderscoreUse,
}

/// The symbols of `(ARGS . BODY)` (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClosureFacts {
    pub(crate) symbols: Vec<SymRole>,
}

/// How a symbol occurs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Occurrence {
    Head,
    Use,
    Binder,
}

#[derive(Clone, Copy)]
enum Walk {
    /// A value in form position.
    Form(Value),
    /// A list whose elements are forms (a body, a call's arguments).
    Forms(Value),
    /// A lambda list: its symbols are binders.
    Binders(Value),
    /// A `let`/`let*` binding list: `VAR` or `(VAR VALUE...)`.
    LetBindings(Value),
    /// `condition-case` handlers: `(CONDITIONS BODY...)`, CONDITIONS data.
    Handlers(Value),
    /// `cond` clauses: each a list of forms.
    Clauses(Value),
}

cached_symbol_id!(condition_case_shape_symbol, "condition-case");
cached_symbol_id!(cond_shape_symbol, "cond");

struct FactsBuilder {
    facts: ClosureFacts,
    index: FxHashMap<SymId, usize>,
    steps: usize,
}

impl FactsBuilder {
    fn step(&mut self) -> Result<(), FactsRefusal> {
        self.steps += 1;
        if self.steps > SHAPE_NODE_CAP {
            Err(FactsRefusal::TooLarge)
        } else {
            Ok(())
        }
    }

    fn note(&mut self, value: Value, occurrence: Occurrence) -> Result<(), FactsRefusal> {
        let Some(id) = value.as_symbol_id() else {
            return Ok(());
        };
        if value.is_nil() {
            return Ok(());
        }
        let head = occurrence == Occurrence::Head;
        match self.index.get(&id) {
            Some(&at) => self.facts.symbols[at].head |= head,
            None => {
                self.index.insert(id, self.facts.symbols.len());
                self.facts.symbols.push(SymRole { id, head });
            }
        }
        if occurrence != Occurrence::Binder && resolve_sym(id).starts_with('_') {
            return Err(FactsRefusal::UnderscoreUse);
        }
        Ok(())
    }

    /// Push each element of LIST (and a non-nil dotted tail) as MAKE(elt).
    fn push_elements(
        &mut self,
        stack: &mut Vec<Walk>,
        list: Value,
        make: fn(Value) -> Walk,
    ) -> Result<(), FactsRefusal> {
        let mut tail = list;
        while tail.is_cons() {
            self.step()?;
            stack.push(make(tail.cons_car()));
            tail = tail.cons_cdr();
        }
        if !tail.is_nil() {
            stack.push(make(tail));
        }
        Ok(())
    }
}

impl ClosureFacts {
    pub(crate) fn of(args: Value, body: Value) -> Result<Self, FactsRefusal> {
        let mut b = FactsBuilder {
            facts: ClosureFacts::default(),
            index: FxHashMap::default(),
            steps: 0,
        };
        let mut stack: Vec<Walk> = vec![Walk::Forms(body), Walk::Binders(args)];
        while let Some(item) = stack.pop() {
            b.step()?;
            match item {
                Walk::Binders(list) => {
                    let mut tail = list;
                    while tail.is_cons() {
                        b.step()?;
                        let var = tail.cons_car();
                        if var.is_cons() {
                            stack.push(Walk::Form(var));
                        } else {
                            b.note(var, Occurrence::Binder)?;
                        }
                        tail = tail.cons_cdr();
                    }
                    b.note(tail, Occurrence::Binder)?;
                }
                Walk::Forms(list) => b.push_elements(&mut stack, list, Walk::Form)?,
                Walk::Clauses(list) => b.push_elements(&mut stack, list, Walk::Forms)?,
                Walk::LetBindings(list) => {
                    let mut tail = list;
                    while tail.is_cons() {
                        b.step()?;
                        let binding = tail.cons_car();
                        if binding.is_cons() {
                            b.note(binding.cons_car(), Occurrence::Binder)?;
                            if binding.cons_car().is_cons() {
                                stack.push(Walk::Form(binding.cons_car()));
                            }
                            stack.push(Walk::Forms(binding.cons_cdr()));
                        } else {
                            b.note(binding, Occurrence::Binder)?;
                        }
                        tail = tail.cons_cdr();
                    }
                    if !tail.is_nil() {
                        stack.push(Walk::Form(tail));
                    }
                }
                Walk::Handlers(list) => {
                    let mut tail = list;
                    while tail.is_cons() {
                        b.step()?;
                        let handler = tail.cons_car();
                        if handler.is_cons() {
                            // The conditions are data; the rest is a body.
                            stack.push(Walk::Forms(handler.cons_cdr()));
                        } else {
                            stack.push(Walk::Form(handler));
                        }
                        tail = tail.cons_cdr();
                    }
                    if !tail.is_nil() {
                        stack.push(Walk::Form(tail));
                    }
                }
                Walk::Form(form) => {
                    if !form.is_cons() {
                        b.note(form, Occurrence::Use)?;
                        continue;
                    }
                    let head = form.cons_car();
                    let rest = form.cons_cdr();
                    let head_id = if head.is_nil() {
                        None
                    } else {
                        head.as_symbol_id()
                    };
                    match head_id {
                        Some(id) if id == quote_symbol() => {}
                        Some(id) if id == function_symbol() => {
                            let target = if rest.is_cons() {
                                rest.cons_car()
                            } else {
                                Value::NIL
                            };
                            if target.is_cons()
                                && target.cons_car().as_symbol_id() == Some(lambda_shape_symbol())
                            {
                                let lambda_rest = target.cons_cdr();
                                if lambda_rest.is_cons() {
                                    stack.push(Walk::Forms(lambda_rest.cons_cdr()));
                                    stack.push(Walk::Binders(lambda_rest.cons_car()));
                                } else {
                                    stack.push(Walk::Form(lambda_rest));
                                }
                            }
                            // (function SYMBOL) and malformed ones: data.
                        }
                        Some(id) if id == let_symbol() || id == let_star_symbol() => {
                            b.note(head, Occurrence::Head)?;
                            if rest.is_cons() {
                                stack.push(Walk::Forms(rest.cons_cdr()));
                                stack.push(Walk::LetBindings(rest.cons_car()));
                            } else {
                                stack.push(Walk::Form(rest));
                            }
                        }
                        Some(id) if id == condition_case_shape_symbol() => {
                            b.note(head, Occurrence::Head)?;
                            if rest.is_cons() && rest.cons_cdr().is_cons() {
                                let var = rest.cons_car();
                                let after = rest.cons_cdr();
                                if var.is_cons() {
                                    stack.push(Walk::Form(var));
                                } else {
                                    b.note(var, Occurrence::Binder)?;
                                }
                                stack.push(Walk::Handlers(after.cons_cdr()));
                                stack.push(Walk::Form(after.cons_car()));
                            } else {
                                stack.push(Walk::Forms(rest));
                            }
                        }
                        Some(id) if id == cond_shape_symbol() => {
                            b.note(head, Occurrence::Head)?;
                            stack.push(Walk::Clauses(rest));
                        }
                        Some(_) => {
                            b.note(head, Occurrence::Head)?;
                            stack.push(Walk::Forms(rest));
                        }
                        None => {
                            stack.push(Walk::Forms(rest));
                            stack.push(Walk::Form(head));
                        }
                    }
                }
            }
        }
        Ok(b.facts)
    }
}

/// The lexical environment as `cconv-make-interpreted-closure` splits it
/// (cconv.el:931 and :962): the cars of its cons entries in order, and its
/// bare symbol entries in order.  Duplicates are kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EnvSummary {
    pub(crate) lex: SmallVec<[SymId; 8]>,
    pub(crate) dynamic: SmallVec<[SymId; 4]>,
}

/// Most entries an environment may have and still be memoized.
pub(crate) const ENV_ENTRY_CAP: usize = 4096;

impl EnvSummary {
    /// `None` for anything but a proper list of `(SYMBOL . VALUE)` entries
    /// and bare non-nil symbols, which the memo leaves to Lisp.
    pub(crate) fn of(env: Value) -> Option<Self> {
        let mut summary = Self::default();
        let mut tail = env;
        let mut entries = 0usize;
        while tail.is_cons() {
            entries += 1;
            if entries > ENV_ENTRY_CAP {
                return None;
            }
            let entry = tail.cons_car();
            if entry.is_cons() {
                let var = entry.cons_car();
                if var.is_nil() {
                    return None;
                }
                summary.lex.push(var.as_symbol_id()?);
            } else if !entry.is_nil() {
                summary.dynamic.push(entry.as_symbol_id()?);
            } else {
                return None;
            }
            tail = tail.cons_cdr();
        }
        tail.is_nil().then_some(summary)
    }
}
