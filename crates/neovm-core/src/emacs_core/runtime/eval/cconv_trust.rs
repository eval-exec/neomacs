//! The trusted set: the code a clean `cconv-make-interpreted-closure` run
//! executes, snapshotted so the memo can tell when it changed (P4.1 S0.4).
//!
//! A memoized closure is exact only while the Lisp that would have computed
//! it is the audited GNU code: `cconv.el` and `macroexp.el` as dumped, and
//! the primitives they call.  [`TRUSTED_LISP`] lists the Lisp functions a
//! clean run -- an identity expansion with no macro, no compiler macro and no
//! warning -- can enter; [`EXCLUDED_CALLEES`] lists the ones their constants
//! name but only other paths reach, each with its reason.  Building the set
//! walks every trusted function's constants (nested byte-code included) and
//! requires each function-bound symbol there to be a subr, a trusted
//! function, an excluded one or a macro; anything else -- a callee a GNU
//! sync added -- leaves the set unbuilt, which disables the memo.
//!
//! The snapshot keeps each trusted symbol's function cell (and each callee
//! subr's).  It is valid while every cell is still `eq` to its snapshot:
//! checked for free while `function_epoch` stands still and cell by cell
//! when it moves.  `advice-add`, `debug-on-entry`, `trace-function`, edebug
//! and redefinition all replace a cell, so the memo stops serving until the
//! original object is back (`advice-remove` restores it).
//!
//! [`TRUSTED_VARIABLES`] classifies every special variable the trusted
//! constants mention; the audit test pins both tables against the dumped
//! byte-code, so a GNU sync that changes what the trusted code reads fails
//! loudly.

use super::*;

/// The Lisp functions a clean run can enter (macroexp.el, cconv.el and the
/// small helpers they call).
pub(crate) const TRUSTED_LISP: &[&str] = &[
    "cconv-make-interpreted-closure",
    "macroexpand-all",
    "macroexp--expand-all",
    "macroexp--all-forms",
    "macroexp--all-clauses",
    "macroexp--cons",
    "macroexp-macroexpand",
    "macroexpand-1",
    "macroexp-warn-and-return",
    "macroexp-compiling-p",
    "macroexp-unprogn",
    "macroexp-const-p",
    "macrop",
    "function-get",
    "special-form-p",
    "booleanp",
    "caar",
    "delete-dups",
    "cconv-fv",
    "cconv-analyze-form",
    "cconv--analyze-function",
    "cconv--analyze-use",
    "cconv--not-lexical-var-p",
    "cl--assertion-failed",
];

/// Why a callee named in the trusted constants is never entered by a run
/// the memo records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExclusionReason {
    /// Raises or debugs an error: a run that reaches it signals, and an
    /// erroring run is never recorded.
    ErrorPath,
    /// `(funcall #'(lambda ...))` / `((lambda ...) ...)`: unfolded into a
    /// `let`, never an identity expansion.
    LambdaUnfolding,
    /// Only for a head with a compiler macro, which the memo refuses.
    CompilerMacro,
    /// Only after a macro expanded (obsolescence warnings), which the memo
    /// refuses.
    MacroExpanded,
    /// Only for a form with symbol positions, which the memo refuses.
    Positions,
    /// Only while compiling (`macroexpand-all-environment` non-nil, which
    /// the memo refuses), or for a warning that is not compile-only: every
    /// warning `macroexp--expand-all` raises itself is compile-only.
    CompilerWarning,
    /// Only for a used `_` variable, which the memo refuses.
    UnderscoreWarning,
}

/// Callees the trusted constants name that clean runs never enter.
pub(crate) const EXCLUDED_CALLEES: &[(&str, ExclusionReason)] = &[
    ("error", ExclusionReason::ErrorPath),
    ("debug", ExclusionReason::ErrorPath),
    ("macroexp--unfold-lambda", ExclusionReason::LambdaUnfolding),
    ("macroexp--compiler-macro", ExclusionReason::CompilerMacro),
    ("macroexp--obsolete-warning", ExclusionReason::MacroExpanded),
    ("macroexp--posify-form", ExclusionReason::Positions),
    ("macroexp--warn-wrap", ExclusionReason::CompilerWarning),
    ("file-relative-name", ExclusionReason::CompilerWarning),
    (
        "byte-compile-warning-enabled-p",
        ExclusionReason::UnderscoreWarning,
    ),
];

/// How the memo accounts for a special variable the trusted code mentions.
/// The audit table; the memo reads the key and bypass variables directly.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrustedVariableRole {
    /// Part of the memo key (`lexical-binding`, buffer-local aware).
    Key,
    /// The memo does not run while it is non-nil.
    Bypass,
    /// Let-bound by the trusted code around every read.
    BoundInRun,
    /// Read only on a path the memo refuses (see [`ExclusionReason`]).
    RefusedPath,
    /// `t`.
    Constant,
}

/// Every special variable named in the trusted constants.
#[cfg(test)]
pub(crate) const TRUSTED_VARIABLES: &[(&str, TrustedVariableRole)] = &[
    ("lexical-binding", TrustedVariableRole::Key),
    ("macroexpand-all-environment", TrustedVariableRole::Bypass),
    ("macroexp--dynvars", TrustedVariableRole::BoundInRun),
    ("byte-compile-form-stack", TrustedVariableRole::BoundInRun),
    ("cconv--dynbound-variables", TrustedVariableRole::BoundInRun),
    (
        "internal-make-interpreted-closure-function",
        TrustedVariableRole::BoundInRun,
    ),
    // `(interactive NONCONST)' in a nested lambda (cconv.el:776).
    (
        "cconv--interactive-form-funs",
        TrustedVariableRole::RefusedPath,
    ),
    // macroexp-preserve-posification, positions only.
    (
        "macroexp-enable-preserve-posification",
        TrustedVariableRole::RefusedPath,
    ),
    // macroexp-warn-and-return while compiling / for a message.
    ("macroexp--warned", TrustedVariableRole::RefusedPath),
    ("load-file-name", TrustedVariableRole::RefusedPath),
    // cl--assertion-failed.
    ("debug-on-error", TrustedVariableRole::RefusedPath),
    ("debugger", TrustedVariableRole::RefusedPath),
    ("t", TrustedVariableRole::Constant),
];

/// Why the trusted set could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TrustRefusal {
    /// A trusted function is void or not byte-code (loaded from source,
    /// redefined, advised before the first use).
    NotCompiled(&'static str),
    /// A trusted function's constants name a function that is neither a
    /// subr, trusted, excluded nor a macro.
    UnauditedCallee(String),
    /// `cconv-make-interpreted-closure` has no `(t)` constant.
    NoEmptyEnvConstant,
}

/// Whether the snapshot currently stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TrustState {
    /// Not built yet (first use builds it).
    Unbuilt,
    /// Built; every cell was `eq` to its snapshot at `verified_epoch`.
    Valid,
    /// Built, but a cell differed when last checked at `checked_epoch`.
    Changed,
    /// Could not be built at `checked_epoch`.
    Refused(TrustRefusal),
}

/// The snapshot (see the module docs).  Its values are GC roots
/// ([`TrustedSet::trace_roots`]).
#[derive(Debug)]
pub(crate) struct TrustedSet {
    state: TrustState,
    /// (symbol, function cell) for the trusted functions and their subrs.
    cells: Vec<(SymId, Value)>,
    /// `cconv-make-interpreted-closure`'s `(t)` constant: the environment
    /// of a closure that captures nothing (cconv.el:972).
    empty_env: Value,
    /// `function_epoch` at the last check.
    checked_epoch: u64,
}

impl Default for TrustedSet {
    fn default() -> Self {
        Self {
            state: TrustState::Unbuilt,
            cells: Vec::new(),
            empty_env: Value::NIL,
            checked_epoch: 0,
        }
    }
}

impl TrustedSet {
    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        for (_, cell) in &self.cells {
            visit(*cell);
        }
        if !self.empty_env.is_nil() {
            visit(self.empty_env);
        }
    }

    pub(crate) fn empty_env(&self) -> Value {
        self.empty_env
    }

    #[cfg(test)]
    pub(crate) fn trusted_cells(&self) -> &[(SymId, Value)] {
        &self.cells
    }

    #[cfg(test)]
    pub(crate) fn refusal(&self) -> Option<&TrustRefusal> {
        match &self.state {
            TrustState::Refused(why) => Some(why),
            _ => None,
        }
    }
}

fn is_subr_value(value: Value) -> bool {
    matches!(
        value.kind(),
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr)
    )
}

fn is_macro_definition(value: Value) -> bool {
    value.is_cons() && value.cons_car().is_symbol_named("macro")
}

/// Push every symbol reachable in CONSTANT (conses, vectors, nested
/// byte-code constants) onto OUT.
fn collect_constant_symbols(constant: Value, out: &mut Vec<SymId>, budget: &mut usize) {
    let mut stack = vec![constant];
    while let Some(value) = stack.pop() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        match value.kind() {
            ValueKind::Symbol(id) => out.push(id),
            ValueKind::Cons => {
                stack.push(value.cons_cdr());
                stack.push(value.cons_car());
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                if let Some(code) = value.get_bytecode_data() {
                    stack.extend(code.constants.iter().copied());
                }
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                if let Some(items) = value.as_vector_data() {
                    stack.extend(items.iter().copied());
                }
            }
            _ => {}
        }
    }
}

impl Context {
    fn trusted_fbound_cell(&self, id: SymId) -> Option<Value> {
        self.obarray
            .symbol_function_id(id)
            .filter(|cell| !cell.is_nil())
    }

    /// Build the snapshot from the current function cells.
    fn build_trusted_set(&self) -> Result<(Vec<(SymId, Value)>, Value), TrustRefusal> {
        let trusted: FxHashMap<SymId, &'static str> = TRUSTED_LISP
            .iter()
            .map(|name| (intern(name), *name))
            .collect();
        let excluded: HashSet<SymId> = EXCLUDED_CALLEES
            .iter()
            .map(|(name, _)| intern(name))
            .collect();
        let mut cells: Vec<(SymId, Value)> = Vec::new();
        let mut seen: HashSet<SymId> = HashSet::new();
        let mut constant_symbols: Vec<SymId> = Vec::new();
        for name in TRUSTED_LISP {
            let id = intern(name);
            let cell = self
                .trusted_fbound_cell(id)
                .filter(|cell| cell.get_bytecode_data().is_some())
                .ok_or(TrustRefusal::NotCompiled(name))?;
            cells.push((id, cell));
            seen.insert(id);
            let mut budget = 1 << 16;
            let code = cell.get_bytecode_data().expect("checked above");
            for constant in code.constants.iter().copied() {
                collect_constant_symbols(constant, &mut constant_symbols, &mut budget);
            }
        }
        for id in constant_symbols {
            if !seen.insert(id) || trusted.contains_key(&id) {
                continue;
            }
            let Some(cell) = self.trusted_fbound_cell(id) else {
                continue;
            };
            if is_subr_value(cell) {
                cells.push((id, cell));
            } else if excluded.contains(&id) || is_macro_definition(cell) {
                continue;
            } else {
                return Err(TrustRefusal::UnauditedCallee(resolve_sym(id).to_string()));
            }
        }
        let cconv = cells[0].1.get_bytecode_data().expect("trusted byte-code");
        let empty_env = cconv
            .constants
            .iter()
            .copied()
            .find(|c| c.is_cons() && c.cons_car().is_t() && c.cons_cdr().is_nil())
            .ok_or(TrustRefusal::NoEmptyEnvConstant)?;
        Ok((cells, empty_env))
    }

    /// Whether the trusted set stands (V1), building it on first use.
    /// Free while `function_epoch` has not moved since the last check.
    pub(crate) fn cconv_trusted_set_valid(&mut self) -> bool {
        let epoch = self.obarray.function_epoch();
        let trusted = &self.cconv_memo.trusted;
        if trusted.checked_epoch == epoch && trusted.state != TrustState::Unbuilt {
            return trusted.state == TrustState::Valid;
        }
        self.cconv_recheck_trusted_set(epoch)
    }

    #[cold]
    #[inline(never)]
    fn cconv_recheck_trusted_set(&mut self, epoch: u64) -> bool {
        let state = match &self.cconv_memo.trusted.state {
            TrustState::Unbuilt | TrustState::Refused(_) => match self.build_trusted_set() {
                Ok((cells, empty_env)) => {
                    let trusted = &mut self.cconv_memo.trusted;
                    trusted.cells = cells;
                    trusted.empty_env = empty_env;
                    TrustState::Valid
                }
                Err(why) => {
                    tracing::debug!(target: "neovm::cconv_memo", ?why, "trusted set refused");
                    TrustState::Refused(why)
                }
            },
            TrustState::Valid | TrustState::Changed => {
                let intact = self.cconv_memo.trusted.cells.iter().all(|(id, cell)| {
                    self.obarray
                        .symbol_function_id(*id)
                        .is_some_and(|now| now == *cell)
                });
                if intact {
                    TrustState::Valid
                } else {
                    TrustState::Changed
                }
            }
        };
        let trusted = &mut self.cconv_memo.trusted;
        trusted.state = state;
        trusted.checked_epoch = epoch;
        trusted.state == TrustState::Valid
    }
}
