//! Advanced CL-lib features: cl-defstruct, cl-loop, cl-destructuring-bind,
//! and additional Common Lisp compatibility special forms and macros.
//!
//! All public functions in this module are special forms that receive unevaluated
//! arguments (as `&[Expr]`) and the evaluator reference.

use std::collections::HashMap;
use std::sync::Arc;

use super::error::{signal, EvalResult, Flow};
use super::eval::Evaluator;
use super::expr::Expr;
use super::value::*;

// ---------------------------------------------------------------------------
// Argument / conversion helpers
// ---------------------------------------------------------------------------

fn expect_symbol(expr: &Expr) -> Result<&str, Flow> {
    match expr {
        Expr::Symbol(s) => Ok(s.as_str()),
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("symbolp")])),
    }
}

fn value_as_symbol(val: &Value) -> Result<&str, Flow> {
    val.as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), val.clone()]))
}

fn expr_to_symbol_string(expr: &Expr) -> Result<String, Flow> {
    match expr {
        Expr::Symbol(s) => Ok(s.clone()),
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("symbolp")])),
    }
}

// ===========================================================================
// 1. cl-defstruct
// ===========================================================================

/// Information about a single struct slot.
struct SlotDef {
    name: String,
    default: Option<Expr>,
}

/// Parse a slot definition: either a bare symbol or (SLOT-NAME DEFAULT-VALUE).
fn parse_slot(expr: &Expr) -> Result<SlotDef, Flow> {
    match expr {
        Expr::Symbol(name) => Ok(SlotDef {
            name: name.clone(),
            default: None,
        }),
        Expr::List(items) if !items.is_empty() => {
            let name = expr_to_symbol_string(&items[0])?;
            let default = if items.len() > 1 {
                Some(items[1].clone())
            } else {
                None
            };
            Ok(SlotDef { name, default })
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::string("Invalid slot definition")],
        )),
    }
}

/// `(cl-defstruct NAME SLOTS...)`
///
/// Creates:
/// - `make-NAME` constructor
/// - `NAME-p` predicate
/// - `NAME-SLOT` accessor for each slot
/// - `setf-NAME-SLOT` setter for each slot
/// - `copy-NAME` copier
///
/// The struct is represented as a vector: `[cl-struct-NAME slot0 slot1 ...]`
/// Struct metadata is stored as a plist property `cl-struct-slots` on the NAME symbol.
pub(crate) fn sf_cl_defstruct(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("cl-defstruct")],
        ));
    }

    // Parse name — can be a bare symbol or (NAME OPTIONS...) but we only support bare symbol.
    let struct_name = match &tail[0] {
        Expr::Symbol(name) => name.clone(),
        Expr::List(items) if !items.is_empty() => {
            // (NAME :option value ...) — just take the name, ignore options for now
            expr_to_symbol_string(&items[0])?
        }
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::string("cl-defstruct: NAME must be a symbol")],
            ));
        }
    };

    // Parse slot definitions
    let mut slots = Vec::new();
    for slot_expr in &tail[1..] {
        // Skip docstring if present
        if let Expr::Str(_) = slot_expr {
            continue;
        }
        slots.push(parse_slot(slot_expr)?);
    }

    let type_tag = Value::symbol(format!("cl-struct-{}", struct_name));
    let slot_names: Vec<String> = slots.iter().map(|s| s.name.clone()).collect();
    let num_slots = slots.len();

    // Store struct metadata on the symbol's plist
    let slot_name_list = Value::list(slot_names.iter().map(|n| Value::symbol(n.clone())).collect());
    eval.obarray
        .put_property(&struct_name, "cl-struct-slots", slot_name_list);
    eval.obarray
        .put_property(&struct_name, "cl-struct-type", type_tag.clone());

    // ----- make-NAME constructor -----
    // Creates a lambda that accepts keyword arguments for each slot.
    // For simplicity, the constructor takes positional &optional arguments matching slot order,
    // OR keyword-style arguments in a &rest list.
    {
        let constructor_name = format!("make-{}", struct_name);
        let _defaults: Vec<Option<Expr>> = slots.iter().map(|s| s.default.clone()).collect();
        let n = num_slots;

        // Build a lambda with &rest args, then parse keywords inside.
        // We register a Subr-like function by creating a Lambda that handles keyword args.
        let slot_names_inner = slot_names.clone();

        // We'll build a native lambda body that constructs the vector.
        // Strategy: build a progn body that:
        //   1. Creates local bindings for each slot with defaults
        //   2. Parses keyword args from the &rest parameter
        //   3. Returns a vector [type-tag slot0 slot1 ...]
        //
        // Since we can't easily create native closures, we generate Lisp code.

        // let-bindings for defaults
        let mut let_bindings = Vec::new();
        for (i, slot) in slots.iter().enumerate() {
            let var = Expr::Symbol(format!("--slot-{}", i));
            let default_expr = slot.default.clone().unwrap_or(Expr::Symbol("nil".into()));
            let_bindings.push(Expr::List(vec![var, default_expr]));
        }

        // Parse keyword args: iterate over the &rest list two at a time
        // (let ((--i 0))
        //   (while (< --i (length --args))
        //     (let ((--key (nth --i --args))
        //           (--val (nth (1+ --i) --args)))
        //       (cond ((eq --key :slot0) (setq --slot-0 --val)) ...)
        //       (setq --i (+ --i 2)))))
        let mut cond_clauses = Vec::new();
        for (i, sn) in slot_names_inner.iter().enumerate() {
            let keyword = Expr::Keyword(format!(":{}", sn));
            cond_clauses.push(Expr::List(vec![
                Expr::List(vec![
                    Expr::Symbol("eq".into()),
                    Expr::Symbol("--key".into()),
                    keyword,
                ]),
                Expr::List(vec![
                    Expr::Symbol("setq".into()),
                    Expr::Symbol(format!("--slot-{}", i)),
                    Expr::Symbol("--val".into()),
                ]),
            ]));
        }

        let parse_loop = Expr::List(vec![
            Expr::Symbol("let".into()),
            Expr::List(vec![Expr::List(vec![
                Expr::Symbol("--i".into()),
                Expr::Int(0),
            ])]),
            Expr::List(vec![
                Expr::Symbol("while".into()),
                Expr::List(vec![
                    Expr::Symbol("<".into()),
                    Expr::Symbol("--i".into()),
                    Expr::List(vec![
                        Expr::Symbol("length".into()),
                        Expr::Symbol("--args".into()),
                    ]),
                ]),
                Expr::List(vec![
                    Expr::Symbol("let".into()),
                    Expr::List(vec![
                        Expr::List(vec![
                            Expr::Symbol("--key".into()),
                            Expr::List(vec![
                                Expr::Symbol("nth".into()),
                                Expr::Symbol("--i".into()),
                                Expr::Symbol("--args".into()),
                            ]),
                        ]),
                        Expr::List(vec![
                            Expr::Symbol("--val".into()),
                            Expr::List(vec![
                                Expr::Symbol("nth".into()),
                                Expr::List(vec![
                                    Expr::Symbol("1+".into()),
                                    Expr::Symbol("--i".into()),
                                ]),
                                Expr::Symbol("--args".into()),
                            ]),
                        ]),
                    ]),
                    {
                        let mut cond = vec![Expr::Symbol("cond".into())];
                        cond.extend(cond_clauses.clone());
                        Expr::List(cond)
                    },
                    Expr::List(vec![
                        Expr::Symbol("setq".into()),
                        Expr::Symbol("--i".into()),
                        Expr::List(vec![
                            Expr::Symbol("+".into()),
                            Expr::Symbol("--i".into()),
                            Expr::Int(2),
                        ]),
                    ]),
                ]),
            ]),
        ]);

        // Build the result vector expression
        let mut vector_args = vec![
            Expr::Symbol("list".into()),
            Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::Symbol(format!("cl-struct-{}", struct_name)),
            ]),
        ];
        for i in 0..n {
            vector_args.push(Expr::Symbol(format!("--slot-{}", i)));
        }
        let result_vec = Expr::List(vec![
            Expr::Symbol("apply".into()),
            Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::Symbol("vector".into()),
            ]),
            {
                let mut list_form = vec![Expr::Symbol("list".into())];
                list_form.push(Expr::List(vec![
                    Expr::Symbol("quote".into()),
                    Expr::Symbol(format!("cl-struct-{}", struct_name)),
                ]));
                for i in 0..n {
                    list_form.push(Expr::Symbol(format!("--slot-{}", i)));
                }
                Expr::List(list_form)
            },
        ]);

        // Full body: (let ((--slot-0 default0) ...) parse-loop result-vec)
        let full_body = Expr::List(vec![
            Expr::Symbol("let".into()),
            Expr::List(let_bindings),
            parse_loop,
            result_vec,
        ]);

        // Build lambda: (lambda (&rest --args) full_body)
        let lambda = LambdaData {
            params: LambdaParams {
                required: vec![],
                optional: vec![],
                rest: Some("--args".into()),
            },
            body: vec![full_body],
            env: None,
            docstring: None,
        };
        eval.obarray
            .set_symbol_function(&constructor_name, Value::Lambda(Arc::new(lambda)));
    }

    // ----- NAME-p predicate -----
    {
        let pred_name = format!("{}-p", struct_name);
        let tag_sym = format!("cl-struct-{}", struct_name);
        // (lambda (obj) (and (vectorp obj) (> (length obj) 0) (eq (aref obj 0) 'TAG)))
        let body = Expr::List(vec![
            Expr::Symbol("and".into()),
            Expr::List(vec![
                Expr::Symbol("vectorp".into()),
                Expr::Symbol("obj".into()),
            ]),
            Expr::List(vec![
                Expr::Symbol(">".into()),
                Expr::List(vec![
                    Expr::Symbol("length".into()),
                    Expr::Symbol("obj".into()),
                ]),
                Expr::Int(0),
            ]),
            Expr::List(vec![
                Expr::Symbol("eq".into()),
                Expr::List(vec![
                    Expr::Symbol("aref".into()),
                    Expr::Symbol("obj".into()),
                    Expr::Int(0),
                ]),
                Expr::List(vec![
                    Expr::Symbol("quote".into()),
                    Expr::Symbol(tag_sym),
                ]),
            ]),
        ]);
        let lambda = LambdaData {
            params: LambdaParams {
                required: vec!["obj".into()],
                optional: vec![],
                rest: None,
            },
            body: vec![body],
            env: None,
            docstring: None,
        };
        eval.obarray
            .set_symbol_function(&pred_name, Value::Lambda(Arc::new(lambda)));
    }

    // ----- NAME-SLOT accessors and setf-NAME-SLOT setters -----
    for (i, slot_name) in slot_names.iter().enumerate() {
        let idx = (i + 1) as i64; // slot 0 is the type tag

        // Accessor: (lambda (obj) (aref obj IDX))
        let accessor_name = format!("{}-{}", struct_name, slot_name);
        let accessor_body = Expr::List(vec![
            Expr::Symbol("aref".into()),
            Expr::Symbol("obj".into()),
            Expr::Int(idx),
        ]);
        let accessor_lambda = LambdaData {
            params: LambdaParams {
                required: vec!["obj".into()],
                optional: vec![],
                rest: None,
            },
            body: vec![accessor_body],
            env: None,
            docstring: None,
        };
        eval.obarray
            .set_symbol_function(&accessor_name, Value::Lambda(Arc::new(accessor_lambda)));

        // Setter: (lambda (obj val) (aset obj IDX val))
        let setter_name = format!("setf-{}-{}", struct_name, slot_name);
        let setter_body = Expr::List(vec![
            Expr::Symbol("aset".into()),
            Expr::Symbol("obj".into()),
            Expr::Int(idx),
            Expr::Symbol("val".into()),
        ]);
        let setter_lambda = LambdaData {
            params: LambdaParams {
                required: vec!["obj".into(), "val".into()],
                optional: vec![],
                rest: None,
            },
            body: vec![setter_body],
            env: None,
            docstring: None,
        };
        eval.obarray
            .set_symbol_function(&setter_name, Value::Lambda(Arc::new(setter_lambda)));
    }

    // ----- copy-NAME copier -----
    {
        let copier_name = format!("copy-{}", struct_name);
        let n_total = (num_slots + 1) as i64;
        // (lambda (obj)
        //   (let ((new (make-vector N nil)))
        //     (dotimes (i N) (aset new i (aref obj i)))
        //     new))
        let body = Expr::List(vec![
            Expr::Symbol("let".into()),
            Expr::List(vec![Expr::List(vec![
                Expr::Symbol("new".into()),
                Expr::List(vec![
                    Expr::Symbol("make-vector".into()),
                    Expr::Int(n_total),
                    Expr::Symbol("nil".into()),
                ]),
            ])]),
            Expr::List(vec![
                Expr::Symbol("dotimes".into()),
                Expr::List(vec![Expr::Symbol("i".into()), Expr::Int(n_total)]),
                Expr::List(vec![
                    Expr::Symbol("aset".into()),
                    Expr::Symbol("new".into()),
                    Expr::Symbol("i".into()),
                    Expr::List(vec![
                        Expr::Symbol("aref".into()),
                        Expr::Symbol("obj".into()),
                        Expr::Symbol("i".into()),
                    ]),
                ]),
            ]),
            Expr::Symbol("new".into()),
        ]);
        let lambda = LambdaData {
            params: LambdaParams {
                required: vec!["obj".into()],
                optional: vec![],
                rest: None,
            },
            body: vec![body],
            env: None,
            docstring: None,
        };
        eval.obarray
            .set_symbol_function(&copier_name, Value::Lambda(Arc::new(lambda)));
    }

    Ok(Value::symbol(struct_name))
}

// ===========================================================================
// 2. cl-loop
// ===========================================================================

/// Parsed representation of a single cl-loop clause.
#[derive(Debug, Clone)]
enum LoopClause {
    /// `for VAR from START to/below END [by STEP]`
    ForFromTo {
        var: String,
        start: Expr,
        end: Expr,
        step: Expr,
        inclusive: bool, // true = `to`, false = `below`
    },
    /// `for VAR downfrom START to/above END [by STEP]`
    ForDownFromTo {
        var: String,
        start: Expr,
        end: Expr,
        step: Expr,
        inclusive: bool,
    },
    /// `for VAR in LIST`
    ForIn {
        var: String,
        list_expr: Expr,
    },
    /// `for VAR across VECTOR`
    ForAcross {
        var: String,
        vec_expr: Expr,
    },
    /// `for VAR = EXPR [then EXPR]`
    ForEquals {
        var: String,
        init_expr: Expr,
        then_expr: Option<Expr>,
    },
    /// `repeat N`
    Repeat {
        count: Expr,
    },
    /// `while CONDITION`
    While {
        condition: Expr,
    },
    /// `until CONDITION`
    Until {
        condition: Expr,
    },
    /// `collect EXPR [into VAR]`
    Collect {
        expr: Expr,
        into: Option<String>,
    },
    /// `append EXPR`
    Append {
        expr: Expr,
    },
    /// `sum EXPR`
    Sum {
        expr: Expr,
    },
    /// `count EXPR`
    Count {
        expr: Expr,
    },
    /// `maximize EXPR`
    Maximize {
        expr: Expr,
    },
    /// `minimize EXPR`
    Minimize {
        expr: Expr,
    },
    /// `do BODY...` (one or more exprs)
    Do {
        body: Vec<Expr>,
    },
    /// `initially BODY...`
    Initially {
        body: Vec<Expr>,
    },
    /// `finally BODY...`
    Finally {
        body: Vec<Expr>,
    },
    /// `return EXPR`
    Return {
        expr: Expr,
    },
    /// `with VAR = EXPR`
    With {
        var: String,
        expr: Expr,
    },
}

/// Parse cl-loop clauses from the tail expressions.
/// The loop body is a flat list of keywords and expressions.
fn parse_loop_clauses(tail: &[Expr]) -> Result<Vec<LoopClause>, Flow> {
    let mut clauses = Vec::new();
    let mut i = 0;

    while i < tail.len() {
        let kw = match &tail[i] {
            Expr::Symbol(s) => s.as_str(),
            _ => {
                return Err(signal(
                    "cl-loop-error",
                    vec![Value::string(format!(
                        "Expected loop keyword, got {:?}",
                        tail[i]
                    ))],
                ));
            }
        };

        match kw {
            "for" | "as" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing variable after 'for'")]));
                }
                let var = expr_to_symbol_string(&tail[i])?;
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing clause after 'for VAR'")]));
                }
                let clause_kw = match &tail[i] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(signal(
                            "cl-loop-error",
                            vec![Value::string("Expected 'from', 'in', 'across', or '=' after 'for VAR'")],
                        ));
                    }
                };
                match clause_kw.as_str() {
                    "from" => {
                        i += 1;
                        if i >= tail.len() {
                            return Err(signal("cl-loop-error", vec![Value::string("Missing start expr")]));
                        }
                        let start = tail[i].clone();
                        i += 1;

                        // Look for to/below/upto/downto
                        let mut end = Expr::Symbol("nil".into());
                        let mut step = Expr::Int(1);
                        let mut inclusive = true;
                        if i < tail.len() {
                            if let Expr::Symbol(s) = &tail[i] {
                                match s.as_str() {
                                    "to" | "upto" => {
                                        inclusive = true;
                                        i += 1;
                                        end = tail.get(i).cloned().unwrap_or(Expr::Symbol("nil".into()));
                                        i += 1;
                                    }
                                    "below" => {
                                        inclusive = false;
                                        i += 1;
                                        end = tail.get(i).cloned().unwrap_or(Expr::Symbol("nil".into()));
                                        i += 1;
                                    }
                                    "downto" => {
                                        inclusive = true;
                                        i += 1;
                                        end = tail.get(i).cloned().unwrap_or(Expr::Symbol("nil".into()));
                                        i += 1;
                                        // Check for by
                                        if i < tail.len() {
                                            if let Expr::Symbol(s) = &tail[i] {
                                                if s == "by" {
                                                    i += 1;
                                                    step = tail.get(i).cloned().unwrap_or(Expr::Int(1));
                                                    i += 1;
                                                }
                                            }
                                        }
                                        clauses.push(LoopClause::ForDownFromTo {
                                            var,
                                            start,
                                            end,
                                            step,
                                            inclusive,
                                        });
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // Check for by
                        if i < tail.len() {
                            if let Expr::Symbol(s) = &tail[i] {
                                if s == "by" {
                                    i += 1;
                                    step = tail.get(i).cloned().unwrap_or(Expr::Int(1));
                                    i += 1;
                                }
                            }
                        }

                        clauses.push(LoopClause::ForFromTo {
                            var,
                            start,
                            end,
                            step,
                            inclusive,
                        });
                    }
                    "downfrom" => {
                        i += 1;
                        if i >= tail.len() {
                            return Err(signal("cl-loop-error", vec![Value::string("Missing start expr")]));
                        }
                        let start = tail[i].clone();
                        i += 1;
                        let mut end = Expr::Int(0);
                        let mut step = Expr::Int(1);
                        let mut inclusive = true;
                        if i < tail.len() {
                            if let Expr::Symbol(s) = &tail[i] {
                                match s.as_str() {
                                    "to" | "downto" => {
                                        inclusive = true;
                                        i += 1;
                                        end = tail.get(i).cloned().unwrap_or(Expr::Int(0));
                                        i += 1;
                                    }
                                    "above" => {
                                        inclusive = false;
                                        i += 1;
                                        end = tail.get(i).cloned().unwrap_or(Expr::Int(0));
                                        i += 1;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if i < tail.len() {
                            if let Expr::Symbol(s) = &tail[i] {
                                if s == "by" {
                                    i += 1;
                                    step = tail.get(i).cloned().unwrap_or(Expr::Int(1));
                                    i += 1;
                                }
                            }
                        }
                        clauses.push(LoopClause::ForDownFromTo {
                            var,
                            start,
                            end,
                            step,
                            inclusive,
                        });
                    }
                    "in" => {
                        i += 1;
                        if i >= tail.len() {
                            return Err(signal("cl-loop-error", vec![Value::string("Missing list expr")]));
                        }
                        let list_expr = tail[i].clone();
                        i += 1;
                        clauses.push(LoopClause::ForIn { var, list_expr });
                    }
                    "across" => {
                        i += 1;
                        if i >= tail.len() {
                            return Err(signal("cl-loop-error", vec![Value::string("Missing vector expr")]));
                        }
                        let vec_expr = tail[i].clone();
                        i += 1;
                        clauses.push(LoopClause::ForAcross { var, vec_expr });
                    }
                    "=" => {
                        i += 1;
                        if i >= tail.len() {
                            return Err(signal("cl-loop-error", vec![Value::string("Missing init expr")]));
                        }
                        let init_expr = tail[i].clone();
                        i += 1;
                        let then_expr = if i < tail.len() {
                            if let Expr::Symbol(s) = &tail[i] {
                                if s == "then" {
                                    i += 1;
                                    if i >= tail.len() {
                                        return Err(signal(
                                            "cl-loop-error",
                                            vec![Value::string("Missing then expr")],
                                        ));
                                    }
                                    let e = tail[i].clone();
                                    i += 1;
                                    Some(e)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        clauses.push(LoopClause::ForEquals {
                            var,
                            init_expr,
                            then_expr,
                        });
                    }
                    other => {
                        return Err(signal(
                            "cl-loop-error",
                            vec![Value::string(format!(
                                "Unknown for-clause keyword: {}",
                                other
                            ))],
                        ));
                    }
                }
            }
            "repeat" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing count after 'repeat'")]));
                }
                clauses.push(LoopClause::Repeat {
                    count: tail[i].clone(),
                });
                i += 1;
            }
            "while" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing condition after 'while'")]));
                }
                clauses.push(LoopClause::While {
                    condition: tail[i].clone(),
                });
                i += 1;
            }
            "until" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing condition after 'until'")]));
                }
                clauses.push(LoopClause::Until {
                    condition: tail[i].clone(),
                });
                i += 1;
            }
            "collect" | "collecting" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'collect'")]));
                }
                let expr = tail[i].clone();
                i += 1;
                let into = if i < tail.len() {
                    if let Expr::Symbol(s) = &tail[i] {
                        if s == "into" {
                            i += 1;
                            if i >= tail.len() {
                                return Err(signal(
                                    "cl-loop-error",
                                    vec![Value::string("Missing var after 'into'")],
                                ));
                            }
                            let v = expr_to_symbol_string(&tail[i])?;
                            i += 1;
                            Some(v)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                clauses.push(LoopClause::Collect { expr, into });
            }
            "append" | "appending" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'append'")]));
                }
                clauses.push(LoopClause::Append {
                    expr: tail[i].clone(),
                });
                i += 1;
            }
            "sum" | "summing" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'sum'")]));
                }
                clauses.push(LoopClause::Sum {
                    expr: tail[i].clone(),
                });
                i += 1;
            }
            "count" | "counting" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'count'")]));
                }
                clauses.push(LoopClause::Count {
                    expr: tail[i].clone(),
                });
                i += 1;
            }
            "maximize" | "maximizing" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'maximize'")]));
                }
                clauses.push(LoopClause::Maximize {
                    expr: tail[i].clone(),
                });
                i += 1;
            }
            "minimize" | "minimizing" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'minimize'")]));
                }
                clauses.push(LoopClause::Minimize {
                    expr: tail[i].clone(),
                });
                i += 1;
            }
            "do" | "doing" => {
                i += 1;
                // Consume body forms until the next loop keyword
                let mut body = Vec::new();
                while i < tail.len() {
                    if is_loop_keyword(&tail[i]) {
                        break;
                    }
                    body.push(tail[i].clone());
                    i += 1;
                }
                if body.is_empty() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing body after 'do'")]));
                }
                clauses.push(LoopClause::Do { body });
            }
            "initially" => {
                i += 1;
                let mut body = Vec::new();
                while i < tail.len() {
                    if is_loop_keyword(&tail[i]) {
                        break;
                    }
                    body.push(tail[i].clone());
                    i += 1;
                }
                clauses.push(LoopClause::Initially { body });
            }
            "finally" => {
                i += 1;
                let mut body = Vec::new();
                while i < tail.len() {
                    if is_loop_keyword(&tail[i]) {
                        break;
                    }
                    body.push(tail[i].clone());
                    i += 1;
                }
                clauses.push(LoopClause::Finally { body });
            }
            "return" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'return'")]));
                }
                clauses.push(LoopClause::Return {
                    expr: tail[i].clone(),
                });
                i += 1;
            }
            "with" => {
                i += 1;
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing var after 'with'")]));
                }
                let var = expr_to_symbol_string(&tail[i])?;
                i += 1;
                // Expect '='
                if i < tail.len() {
                    if let Expr::Symbol(s) = &tail[i] {
                        if s == "=" {
                            i += 1;
                        }
                    }
                }
                if i >= tail.len() {
                    return Err(signal("cl-loop-error", vec![Value::string("Missing expr after 'with VAR ='")]));
                }
                let expr = tail[i].clone();
                i += 1;
                clauses.push(LoopClause::With { var, expr });
            }
            other => {
                return Err(signal(
                    "cl-loop-error",
                    vec![Value::string(format!("Unknown loop keyword: {}", other))],
                ));
            }
        }
    }

    Ok(clauses)
}

fn is_loop_keyword(expr: &Expr) -> bool {
    if let Expr::Symbol(s) = expr {
        matches!(
            s.as_str(),
            "for" | "as" | "repeat" | "while" | "until" | "collect" | "collecting"
                | "append" | "appending" | "sum" | "summing" | "count" | "counting"
                | "maximize" | "maximizing" | "minimize" | "minimizing"
                | "do" | "doing" | "initially" | "finally" | "return" | "with"
        )
    } else {
        false
    }
}

/// `(cl-loop CLAUSES...)`
///
/// The main cl-loop implementation. Parses clauses, then interprets them.
pub(crate) fn sf_cl_loop(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Ok(Value::Nil);
    }

    let clauses = parse_loop_clauses(tail)?;

    // Set up accumulation state
    let mut collect_result: Vec<Value> = Vec::new();
    let mut sum_result: f64 = 0.0;
    let mut sum_is_int = true;
    let mut count_result: i64 = 0;
    let mut max_result: Option<f64> = None;
    let mut min_result: Option<f64> = None;
    let mut has_collect = false;
    let mut has_append = false;
    let mut has_sum = false;
    let mut has_count = false;
    let mut has_maximize = false;
    let mut has_minimize = false;
    let mut finally_clauses: Vec<Vec<Expr>> = Vec::new();
    let mut initially_clauses: Vec<Vec<Expr>> = Vec::new();

    // Detect accumulation types
    for clause in &clauses {
        match clause {
            LoopClause::Collect { .. } => has_collect = true,
            LoopClause::Append { .. } => has_append = true,
            LoopClause::Sum { .. } => has_sum = true,
            LoopClause::Count { .. } => has_count = true,
            LoopClause::Maximize { .. } => has_maximize = true,
            LoopClause::Minimize { .. } => has_minimize = true,
            LoopClause::Finally { body } => finally_clauses.push(body.clone()),
            LoopClause::Initially { body } => initially_clauses.push(body.clone()),
            _ => {}
        }
    }

    // Collect named into-vars
    let mut named_collects: HashMap<String, Vec<Value>> = HashMap::new();
    for clause in &clauses {
        if let LoopClause::Collect { into: Some(name), .. } = clause {
            named_collects.entry(name.clone()).or_default();
        }
    }

    // Push a dynamic frame for loop variables
    eval.dynamic.push(HashMap::new());

    // Initialize `with` and `for` variables
    for clause in &clauses {
        match clause {
            LoopClause::With { var, expr } => {
                let val = eval.eval(expr)?;
                if let Some(frame) = eval.dynamic.last_mut() {
                    frame.insert(var.clone(), val);
                }
            }
            LoopClause::ForFromTo { var, start, .. } => {
                let val = eval.eval(start)?;
                if let Some(frame) = eval.dynamic.last_mut() {
                    frame.insert(var.clone(), val);
                }
            }
            LoopClause::ForDownFromTo { var, start, .. } => {
                let val = eval.eval(start)?;
                if let Some(frame) = eval.dynamic.last_mut() {
                    frame.insert(var.clone(), val);
                }
            }
            LoopClause::ForEquals { var, init_expr, .. } => {
                let val = eval.eval(init_expr)?;
                if let Some(frame) = eval.dynamic.last_mut() {
                    frame.insert(var.clone(), val);
                }
            }
            _ => {}
        }
    }

    // Initialize for-in / for-across iteration state as separate tracking
    // We store internal iterator state outside the dynamic frame.
    struct IterState {
        items: Vec<Value>,
        index: usize,
    }
    let mut for_in_states: Vec<(String, IterState)> = Vec::new();
    let mut for_across_states: Vec<(String, IterState)> = Vec::new();

    for clause in &clauses {
        match clause {
            LoopClause::ForIn { var, list_expr } => {
                let list_val = eval.eval(list_expr)?;
                let items = list_to_vec(&list_val).unwrap_or_default();
                if let Some(first) = items.first() {
                    if let Some(frame) = eval.dynamic.last_mut() {
                        frame.insert(var.clone(), first.clone());
                    }
                }
                for_in_states.push((var.clone(), IterState { items, index: 0 }));
            }
            LoopClause::ForAcross { var, vec_expr } => {
                let vec_val = eval.eval(vec_expr)?;
                let items = match &vec_val {
                    Value::Vector(v) => v.lock().expect("poisoned").clone(),
                    Value::Str(s) => s.chars().map(Value::Char).collect(),
                    _ => Vec::new(),
                };
                if let Some(first) = items.first() {
                    if let Some(frame) = eval.dynamic.last_mut() {
                        frame.insert(var.clone(), first.clone());
                    }
                }
                for_across_states.push((var.clone(), IterState { items, index: 0 }));
            }
            _ => {}
        }
    }

    // Repeat counter
    let mut repeat_remaining: Option<i64> = None;
    for clause in &clauses {
        if let LoopClause::Repeat { count } = clause {
            let n = eval.eval(count)?;
            repeat_remaining = Some(match &n {
                Value::Int(i) => *i,
                Value::Float(f) => *f as i64,
                _ => 0,
            });
        }
    }

    // Execute initially forms
    for body in &initially_clauses {
        for form in body {
            eval.eval(form)?;
        }
    }

    // Main loop
    let mut explicit_return: Option<Value> = None;
    let mut first_iteration = true;

    'outer: loop {
        // Check repeat counter
        if let Some(ref mut remaining) = repeat_remaining {
            if *remaining <= 0 {
                break;
            }
            *remaining -= 1;
        }

        // Update for-in iterators
        for (var, state) in &mut for_in_states {
            if !first_iteration {
                state.index += 1;
            }
            if state.index >= state.items.len() {
                break 'outer;
            }
            if let Some(frame) = eval.dynamic.last_mut() {
                frame.insert(var.clone(), state.items[state.index].clone());
            }
        }

        // Update for-across iterators
        for (var, state) in &mut for_across_states {
            if !first_iteration {
                state.index += 1;
            }
            if state.index >= state.items.len() {
                break 'outer;
            }
            if let Some(frame) = eval.dynamic.last_mut() {
                frame.insert(var.clone(), state.items[state.index].clone());
            }
        }

        // Update for-from-to (on non-first iteration, step the variable)
        if !first_iteration {
            for clause in &clauses {
                match clause {
                    LoopClause::ForFromTo {
                        var, step, ..
                    } => {
                        let step_val = eval.eval(step)?;
                        let current = eval.dynamic.last().and_then(|f| f.get(var)).cloned().unwrap_or(Value::Int(0));
                        let new_val = add_values(&current, &step_val)?;
                        if let Some(frame) = eval.dynamic.last_mut() {
                            frame.insert(var.clone(), new_val);
                        }
                    }
                    LoopClause::ForDownFromTo {
                        var, step, ..
                    } => {
                        let step_val = eval.eval(step)?;
                        let current = eval.dynamic.last().and_then(|f| f.get(var)).cloned().unwrap_or(Value::Int(0));
                        let new_val = sub_values(&current, &step_val)?;
                        if let Some(frame) = eval.dynamic.last_mut() {
                            frame.insert(var.clone(), new_val);
                        }
                    }
                    LoopClause::ForEquals {
                        var,
                        init_expr,
                        then_expr,
                    } => {
                        let expr = then_expr.as_ref().unwrap_or(init_expr);
                        let val = eval.eval(expr)?;
                        if let Some(frame) = eval.dynamic.last_mut() {
                            frame.insert(var.clone(), val);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check for-from-to termination conditions
        for clause in &clauses {
            match clause {
                LoopClause::ForFromTo {
                    var, end, inclusive, ..
                } => {
                    let end_val = eval.eval(end)?;
                    // nil end means no upper bound
                    if !end_val.is_nil() {
                        let current = eval.dynamic.last().and_then(|f| f.get(var)).cloned().unwrap_or(Value::Int(0));
                        if *inclusive {
                            if compare_values(&current, &end_val)? > 0 {
                                break 'outer;
                            }
                        } else if compare_values(&current, &end_val)? >= 0 {
                            break 'outer;
                        }
                    }
                }
                LoopClause::ForDownFromTo {
                    var, end, inclusive, ..
                } => {
                    let end_val = eval.eval(end)?;
                    if !end_val.is_nil() {
                        let current = eval.dynamic.last().and_then(|f| f.get(var)).cloned().unwrap_or(Value::Int(0));
                        if *inclusive {
                            if compare_values(&current, &end_val)? < 0 {
                                break 'outer;
                            }
                        } else if compare_values(&current, &end_val)? <= 0 {
                            break 'outer;
                        }
                    }
                }
                _ => {}
            }
        }

        // Process body clauses
        for clause in &clauses {
            match clause {
                LoopClause::While { condition } => {
                    let val = eval.eval(condition)?;
                    if val.is_nil() {
                        break 'outer;
                    }
                }
                LoopClause::Until { condition } => {
                    let val = eval.eval(condition)?;
                    if val.is_truthy() {
                        break 'outer;
                    }
                }
                LoopClause::Collect { expr, into } => {
                    let val = eval.eval(expr)?;
                    if let Some(name) = into {
                        named_collects.entry(name.clone()).or_default().push(val);
                    } else {
                        collect_result.push(val);
                    }
                }
                LoopClause::Append { expr } => {
                    let val = eval.eval(expr)?;
                    let items = list_to_vec(&val).unwrap_or_default();
                    collect_result.extend(items);
                }
                LoopClause::Sum { expr } => {
                    let val = eval.eval(expr)?;
                    match &val {
                        Value::Int(n) => {
                            sum_result += *n as f64;
                        }
                        Value::Float(f) => {
                            sum_result += *f;
                            sum_is_int = false;
                        }
                        _ => {}
                    }
                }
                LoopClause::Count { expr } => {
                    let val = eval.eval(expr)?;
                    if val.is_truthy() {
                        count_result += 1;
                    }
                }
                LoopClause::Maximize { expr } => {
                    let val = eval.eval(expr)?;
                    if let Some(f) = val.as_number_f64() {
                        max_result = Some(match max_result {
                            Some(cur) => cur.max(f),
                            None => f,
                        });
                    }
                }
                LoopClause::Minimize { expr } => {
                    let val = eval.eval(expr)?;
                    if let Some(f) = val.as_number_f64() {
                        min_result = Some(match min_result {
                            Some(cur) => cur.min(f),
                            None => f,
                        });
                    }
                }
                LoopClause::Do { body } => {
                    for form in body {
                        eval.eval(form)?;
                    }
                }
                LoopClause::Return { expr } => {
                    let val = eval.eval(expr)?;
                    explicit_return = Some(val);
                    break 'outer;
                }
                // Skip iteration/accumulation-only clauses handled above
                _ => {}
            }
        }

        first_iteration = false;
    }

    // Set named collect vars
    for (name, items) in &named_collects {
        let val = Value::list(items.clone());
        eval.assign(name, val);
    }

    // Execute finally forms
    for body in &finally_clauses {
        for form in body {
            eval.eval(form)?;
        }
    }

    // Pop the dynamic frame
    eval.dynamic.pop();

    // Return the result
    if let Some(val) = explicit_return {
        return Ok(val);
    }

    if has_collect || has_append {
        return Ok(Value::list(collect_result));
    }
    if has_sum {
        return if sum_is_int {
            Ok(Value::Int(sum_result as i64))
        } else {
            Ok(Value::Float(sum_result))
        };
    }
    if has_count {
        return Ok(Value::Int(count_result));
    }
    if has_maximize {
        return match max_result {
            Some(f) => Ok(Value::Int(f as i64)),
            None => Ok(Value::Nil),
        };
    }
    if has_minimize {
        return match min_result {
            Some(f) => Ok(Value::Int(f as i64)),
            None => Ok(Value::Nil),
        };
    }

    Ok(Value::Nil)
}

/// Add two numeric values.
fn add_values(a: &Value, b: &Value) -> Result<Value, Flow> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        _ => {
            let fa = a
                .as_number_f64()
                .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), a.clone()]))?;
            let fb = b
                .as_number_f64()
                .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), b.clone()]))?;
            if a.is_integer() && b.is_integer() {
                Ok(Value::Int((fa + fb) as i64))
            } else {
                Ok(Value::Float(fa + fb))
            }
        }
    }
}

/// Subtract two numeric values.
fn sub_values(a: &Value, b: &Value) -> Result<Value, Flow> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
        _ => {
            let fa = a
                .as_number_f64()
                .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), a.clone()]))?;
            let fb = b
                .as_number_f64()
                .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), b.clone()]))?;
            if a.is_integer() && b.is_integer() {
                Ok(Value::Int((fa - fb) as i64))
            } else {
                Ok(Value::Float(fa - fb))
            }
        }
    }
}

/// Compare two numeric values. Returns -1, 0, or 1.
fn compare_values(a: &Value, b: &Value) -> Result<i32, Flow> {
    let fa = a
        .as_number_f64()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), a.clone()]))?;
    let fb = b
        .as_number_f64()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("numberp"), b.clone()]))?;
    if fa < fb {
        Ok(-1)
    } else if fa > fb {
        Ok(1)
    } else {
        Ok(0)
    }
}

// ===========================================================================
// 3. cl-destructuring-bind
// ===========================================================================

/// `(cl-destructuring-bind PATTERN VALUE BODY...)`
///
/// Bind nested list/vector patterns. Supports:
/// - Symbols: bind the value
/// - Lists: destructure recursively (supports &optional, &rest)
/// - Vectors: destructure by index
/// - nil: discard value
pub(crate) fn sf_cl_destructuring_bind(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("cl-destructuring-bind")],
        ));
    }

    let pattern = &tail[0];
    let value = eval.eval(&tail[1])?;
    let body = &tail[2..];

    let mut bindings = HashMap::new();
    destructure_pattern(pattern, &value, &mut bindings)?;

    eval.dynamic.push(bindings);
    let result = eval.sf_progn(body);
    eval.dynamic.pop();
    result
}

/// Recursively destructure a pattern against a value, accumulating bindings.
fn destructure_pattern(
    pattern: &Expr,
    value: &Value,
    bindings: &mut HashMap<String, Value>,
) -> Result<(), Flow> {
    match pattern {
        Expr::Symbol(name) if name == "nil" || name == "_" => {
            // Discard
            Ok(())
        }
        Expr::Symbol(name) => {
            bindings.insert(name.clone(), value.clone());
            Ok(())
        }
        Expr::List(elements) if elements.is_empty() => {
            // Empty pattern — just check value is nil-like
            Ok(())
        }
        Expr::List(elements) => {
            // List destructuring with &optional and &rest support
            let items = list_to_vec(value).unwrap_or_default();
            let mut item_idx = 0;
            let mut pat_idx = 0;
            let mut mode = 0; // 0=required, 1=optional, 2=rest

            while pat_idx < elements.len() {
                let pat = &elements[pat_idx];

                // Check for &optional, &rest keywords
                if let Expr::Symbol(s) = pat {
                    if s == "&optional" {
                        mode = 1;
                        pat_idx += 1;
                        continue;
                    }
                    if s == "&rest" {
                        mode = 2;
                        pat_idx += 1;
                        continue;
                    }
                }

                match mode {
                    0 | 1 => {
                        // Required or optional
                        let val = if item_idx < items.len() {
                            items[item_idx].clone()
                        } else if mode == 1 {
                            Value::Nil
                        } else {
                            return Err(signal(
                                "wrong-number-of-arguments",
                                vec![Value::string(
                                    "Not enough values for destructuring pattern",
                                )],
                            ));
                        };
                        destructure_pattern(pat, &val, bindings)?;
                        item_idx += 1;
                    }
                    2 => {
                        // Rest — bind remaining items as a list
                        let rest_vals: Vec<Value> = items[item_idx..].to_vec();
                        destructure_pattern(pat, &Value::list(rest_vals), bindings)?;
                        item_idx = items.len();
                    }
                    _ => {}
                }
                pat_idx += 1;
            }
            Ok(())
        }
        Expr::Vector(elements) => {
            // Vector destructuring
            let vec_items = match value {
                Value::Vector(v) => v.lock().expect("poisoned").clone(),
                _ => Vec::new(),
            };
            for (i, pat) in elements.iter().enumerate() {
                let val = vec_items.get(i).cloned().unwrap_or(Value::Nil);
                destructure_pattern(pat, &val, bindings)?;
            }
            Ok(())
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::string("Invalid destructuring pattern")],
        )),
    }
}

// ===========================================================================
// 4. Additional CL builtins
// ===========================================================================

// ---- cl-incf / cl-decf ----

/// `(cl-incf PLACE &optional DELTA)`
pub(crate) fn sf_cl_incf(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-incf")]));
    }
    let var_name = expect_symbol(&tail[0])?;
    let delta = if tail.len() > 1 {
        eval.eval(&tail[1])?
    } else {
        Value::Int(1)
    };
    let current = eval.eval(&tail[0])?;
    let new_val = add_values(&current, &delta)?;
    eval.assign(var_name, new_val.clone());
    Ok(new_val)
}

/// `(cl-decf PLACE &optional DELTA)`
pub(crate) fn sf_cl_decf(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-decf")]));
    }
    let var_name = expect_symbol(&tail[0])?;
    let delta = if tail.len() > 1 {
        eval.eval(&tail[1])?
    } else {
        Value::Int(1)
    };
    let current = eval.eval(&tail[0])?;
    let new_val = sub_values(&current, &delta)?;
    eval.assign(var_name, new_val.clone());
    Ok(new_val)
}

// ---- cl-push / cl-pop / cl-pushnew ----

/// `(cl-push ITEM PLACE)` — push an item onto the front of a list variable.
pub(crate) fn sf_cl_push(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() != 2 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-push")]));
    }
    let item = eval.eval(&tail[0])?;
    let var_name = expect_symbol(&tail[1])?;
    let current = eval.eval(&tail[1])?;
    let new_val = Value::cons(item, current);
    eval.assign(var_name, new_val.clone());
    Ok(new_val)
}

/// `(cl-pop PLACE)` — pop the first item from a list variable.
pub(crate) fn sf_cl_pop(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() != 1 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-pop")]));
    }
    let var_name = expect_symbol(&tail[0])?;
    let current = eval.eval(&tail[0])?;
    match &current {
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            let car = pair.car.clone();
            let cdr = pair.cdr.clone();
            drop(pair);
            eval.assign(var_name, cdr);
            Ok(car)
        }
        Value::Nil => Ok(Value::Nil),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), current],
        )),
    }
}

/// `(cl-pushnew ITEM PLACE)` — push item onto list only if not already present (using `equal`).
pub(crate) fn sf_cl_pushnew(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 2 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-pushnew")]));
    }
    let item = eval.eval(&tail[0])?;
    let var_name = expect_symbol(&tail[1])?;
    let current = eval.eval(&tail[1])?;

    // Check if item is already in the list
    let items = list_to_vec(&current).unwrap_or_default();
    let already_present = items.iter().any(|e| equal_value(&item, e, 0));

    if already_present {
        Ok(current)
    } else {
        let new_val = Value::cons(item, current);
        eval.assign(var_name, new_val.clone());
        Ok(new_val)
    }
}

// ---- cl-assert / cl-check-type ----

/// `(cl-assert EXPR &optional STRING)` — signal error if expr is nil.
pub(crate) fn sf_cl_assert(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-assert")]));
    }
    let val = eval.eval(&tail[0])?;
    if val.is_nil() {
        let msg = if tail.len() > 1 {
            let s = eval.eval(&tail[1])?;
            match &s {
                Value::Str(s) => (**s).clone(),
                _ => "Assertion failed".to_string(),
            }
        } else {
            "Assertion failed".to_string()
        };
        Err(signal("cl-assertion-failed", vec![Value::string(msg)]))
    } else {
        Ok(Value::Nil)
    }
}

/// `(cl-check-type VAR TYPE &optional STRING)` — signal error if variable does not have the given type.
pub(crate) fn sf_cl_check_type(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("cl-check-type")],
        ));
    }
    let val = eval.eval(&tail[0])?;
    let type_sym = expect_symbol(&tail[1])?;

    let ok = match type_sym {
        "integer" | "integerp" => val.is_integer(),
        "float" | "floatp" => val.is_float(),
        "number" | "numberp" => val.is_number(),
        "string" | "stringp" => val.is_string(),
        "symbol" | "symbolp" => val.is_symbol(),
        "cons" | "consp" => val.is_cons(),
        "list" | "listp" => val.is_list(),
        "vector" | "vectorp" => val.is_vector(),
        "char" | "characterp" => val.is_char(),
        "function" | "functionp" => val.is_function(),
        "hash-table" | "hash-table-p" => val.is_hash_table(),
        "null" => val.is_nil(),
        _ => true, // Unknown type — pass
    };

    if ok {
        Ok(Value::Nil)
    } else {
        let msg = if tail.len() > 2 {
            let s = eval.eval(&tail[2])?;
            match &s {
                Value::Str(s) => (**s).clone(),
                _ => format!("Type check failed: expected {}", type_sym),
            }
        } else {
            format!("Type check failed: expected {}, got {}", type_sym, val.type_name())
        };
        Err(signal("wrong-type-argument", vec![Value::symbol(type_sym), val, Value::string(msg)]))
    }
}

// ---- cl-case / cl-ecase / cl-typecase / cl-etypecase ----

/// `(cl-case EXPR (KEY BODY...)...)`
pub(crate) fn sf_cl_case(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-case")]));
    }
    let key = eval.eval(&tail[0])?;

    for clause in &tail[1..] {
        let Expr::List(items) = clause else {
            return Err(signal("wrong-type-argument", vec![Value::string("cl-case clause must be a list")]));
        };
        if items.is_empty() {
            continue;
        }

        let pattern = &items[0];
        let body = &items[1..];

        // Check if this is the default clause (t or otherwise)
        let matches = match pattern {
            Expr::Symbol(s) if s == "t" || s == "otherwise" => true,
            Expr::List(keys) => {
                // List of keys: match if any key matches
                keys.iter().any(|k| case_key_matches(&key, k))
            }
            _ => case_key_matches(&key, pattern),
        };

        if matches {
            return eval.sf_progn(body);
        }
    }

    Ok(Value::Nil)
}

/// `(cl-ecase EXPR (KEY BODY...)...)` — like cl-case but signals error if no clause matches.
pub(crate) fn sf_cl_ecase(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-ecase")]));
    }
    let key = eval.eval(&tail[0])?;

    for clause in &tail[1..] {
        let Expr::List(items) = clause else {
            return Err(signal("wrong-type-argument", vec![Value::string("cl-ecase clause must be a list")]));
        };
        if items.is_empty() {
            continue;
        }
        let pattern = &items[0];
        let body = &items[1..];

        let matches = match pattern {
            Expr::List(keys) => keys.iter().any(|k| case_key_matches(&key, k)),
            _ => case_key_matches(&key, pattern),
        };

        if matches {
            return eval.sf_progn(body);
        }
    }

    Err(signal(
        "cl-ecase-error",
        vec![
            Value::string("cl-ecase: no matching clause"),
            key,
        ],
    ))
}

/// `(cl-typecase EXPR (TYPE BODY...)...)`
pub(crate) fn sf_cl_typecase(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-typecase")]));
    }
    let val = eval.eval(&tail[0])?;

    for clause in &tail[1..] {
        let Expr::List(items) = clause else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        if items.is_empty() {
            continue;
        }
        let type_pattern = &items[0];
        let body = &items[1..];

        let matches = match type_pattern {
            Expr::Symbol(s) if s == "t" || s == "otherwise" => true,
            Expr::Symbol(s) => type_matches(&val, s),
            _ => false,
        };

        if matches {
            return eval.sf_progn(body);
        }
    }

    Ok(Value::Nil)
}

/// `(cl-etypecase EXPR (TYPE BODY...)...)` — like cl-typecase but errors if no match.
pub(crate) fn sf_cl_etypecase(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("cl-etypecase")],
        ));
    }
    let val = eval.eval(&tail[0])?;

    for clause in &tail[1..] {
        let Expr::List(items) = clause else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        if items.is_empty() {
            continue;
        }
        let type_pattern = &items[0];
        let body = &items[1..];

        if let Expr::Symbol(s) = type_pattern {
            if type_matches(&val, s) {
                return eval.sf_progn(body);
            }
        }
    }

    Err(signal(
        "cl-etypecase-error",
        vec![
            Value::string("cl-etypecase: no matching clause"),
            val,
        ],
    ))
}

fn case_key_matches(key: &Value, pattern: &Expr) -> bool {
    match pattern {
        Expr::Int(n) => matches!(key, Value::Int(k) if k == n),
        Expr::Symbol(s) if s == "nil" => key.is_nil(),
        Expr::Symbol(s) if s == "t" => matches!(key, Value::True),
        Expr::Symbol(s) => matches!(key, Value::Symbol(k) if k == s),
        Expr::Str(s) => matches!(key, Value::Str(k) if **k == *s),
        Expr::Char(c) => matches!(key, Value::Char(k) if k == c),
        Expr::Keyword(k) => matches!(key, Value::Keyword(kk) if kk == k),
        _ => false,
    }
}

fn type_matches(val: &Value, type_name: &str) -> bool {
    match type_name {
        "integer" | "fixnum" => val.is_integer(),
        "float" => val.is_float(),
        "number" => val.is_number(),
        "string" => val.is_string(),
        "symbol" => val.is_symbol(),
        "cons" => val.is_cons(),
        "list" => val.is_list(),
        "vector" | "array" => val.is_vector(),
        "character" => val.is_char(),
        "function" => val.is_function(),
        "hash-table" => val.is_hash_table(),
        "null" => val.is_nil(),
        "atom" => !val.is_cons(),
        "keyword" => val.is_keyword(),
        "t" | "otherwise" => true,
        _ => false,
    }
}

// ---- cl-block / cl-return-from ----

/// `(cl-block NAME BODY...)` — establish a named lexical block.
/// Uses catch/throw with a generated tag.
pub(crate) fn sf_cl_block(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-block")]));
    }
    let block_name = expect_symbol(&tail[0])?;
    let tag = Value::symbol(format!("--cl-block-{}", block_name));

    match eval.sf_progn(&tail[1..]) {
        Ok(val) => Ok(val),
        Err(Flow::Throw {
            tag: thrown_tag,
            value,
        }) if eq_value(&tag, &thrown_tag) => Ok(value),
        Err(flow) => Err(flow),
    }
}

/// `(cl-return-from NAME &optional VALUE)` — return from a named block.
pub(crate) fn sf_cl_return_from(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("cl-return-from")],
        ));
    }
    let block_name = expect_symbol(&tail[0])?;
    let value = if tail.len() > 1 {
        eval.eval(&tail[1])?
    } else {
        Value::Nil
    };
    let tag = Value::symbol(format!("--cl-block-{}", block_name));
    Err(Flow::Throw { tag, value })
}

// ---- cl-dotimes / cl-dolist ----

/// `(cl-dotimes (VAR COUNT [RESULT]) BODY...)`
pub(crate) fn sf_cl_dotimes(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-dotimes")]));
    }
    let Expr::List(spec) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };
    if spec.len() < 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let var = expr_to_symbol_string(&spec[0])?;
    let count_val = eval.eval(&spec[1])?;
    let count = match &count_val {
        Value::Int(n) => *n,
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integerp"), count_val],
            ));
        }
    };

    eval.dynamic.push(HashMap::new());
    for i in 0..count {
        if let Some(frame) = eval.dynamic.last_mut() {
            frame.insert(var.clone(), Value::Int(i));
        }
        eval.sf_progn(&tail[1..])?;
    }
    let result = if spec.len() > 2 {
        if let Some(frame) = eval.dynamic.last_mut() {
            frame.insert(var.clone(), Value::Int(count));
        }
        eval.eval(&spec[2])?
    } else {
        Value::Nil
    };
    eval.dynamic.pop();
    Ok(result)
}

/// `(cl-dolist (VAR LIST [RESULT]) BODY...)`
pub(crate) fn sf_cl_dolist(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-dolist")]));
    }
    let Expr::List(spec) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };
    if spec.len() < 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let var = expr_to_symbol_string(&spec[0])?;
    let list_val = eval.eval(&spec[1])?;
    let items = list_to_vec(&list_val).unwrap_or_default();

    eval.dynamic.push(HashMap::new());
    for item in items {
        if let Some(frame) = eval.dynamic.last_mut() {
            frame.insert(var.clone(), item);
        }
        eval.sf_progn(&tail[1..])?;
    }
    let result = if spec.len() > 2 {
        if let Some(frame) = eval.dynamic.last_mut() {
            frame.insert(var.clone(), Value::Nil);
        }
        eval.eval(&spec[2])?
    } else {
        Value::Nil
    };
    eval.dynamic.pop();
    Ok(result)
}

// ---- cl-labels / cl-flet ----

/// `(cl-flet ((NAME ARGLIST BODY...) ...) BODY...)`
/// Bind local functions (non-recursive — they don't see each other).
pub(crate) fn sf_cl_flet(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-flet")]));
    }
    let Expr::List(bindings) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };

    // Save old function bindings
    let mut saved: Vec<(String, Option<Value>)> = Vec::new();

    for binding in bindings {
        let Expr::List(items) = binding else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        if items.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let name = expr_to_symbol_string(&items[0])?;
        let lambda = eval.eval_lambda(&items[1..])?;

        saved.push((name.clone(), eval.obarray.symbol_function(&name).cloned()));
        eval.obarray.set_symbol_function(&name, lambda);
    }

    let result = eval.sf_progn(&tail[1..]);

    // Restore old function bindings
    for (name, old_val) in saved.into_iter().rev() {
        match old_val {
            Some(f) => eval.obarray.set_symbol_function(&name, f),
            None => eval.obarray.fmakunbound(&name),
        }
    }

    result
}

/// `(cl-labels ((NAME ARGLIST BODY...) ...) BODY...)`
/// Bind local functions that can call each other (and themselves recursively).
pub(crate) fn sf_cl_labels(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("cl-labels")]));
    }
    let Expr::List(bindings) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };

    // Save old function bindings and install new ones
    let mut saved: Vec<(String, Option<Value>)> = Vec::new();

    // First pass: install all function names (with placeholder lambdas)
    let mut names_and_items: Vec<(String, Vec<Expr>)> = Vec::new();
    for binding in bindings {
        let Expr::List(items) = binding else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        if items.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let name = expr_to_symbol_string(&items[0])?;
        saved.push((name.clone(), eval.obarray.symbol_function(&name).cloned()));
        names_and_items.push((name, items[1..].to_vec()));
    }

    // Second pass: create lambdas (they can now reference each other since all are installed)
    for (name, items) in &names_and_items {
        let lambda = eval.eval_lambda(items)?;
        eval.obarray.set_symbol_function(name, lambda);
    }

    let result = eval.sf_progn(&tail[1..]);

    // Restore old function bindings
    for (name, old_val) in saved.into_iter().rev() {
        match old_val {
            Some(f) => eval.obarray.set_symbol_function(&name, f),
            None => eval.obarray.fmakunbound(&name),
        }
    }

    result
}

// ---- cl-progv ----

/// `(cl-progv SYMBOLS VALUES BODY...)`
/// Dynamic variable binding from lists of symbols and values.
pub(crate) fn sf_cl_progv(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("cl-progv")],
        ));
    }
    let symbols_val = eval.eval(&tail[0])?;
    let values_val = eval.eval(&tail[1])?;

    let syms = list_to_vec(&symbols_val).unwrap_or_default();
    let vals = list_to_vec(&values_val).unwrap_or_default();

    let mut frame = HashMap::new();
    for (i, sym) in syms.iter().enumerate() {
        let name = value_as_symbol(sym)?;
        let val = vals.get(i).cloned().unwrap_or(Value::Nil);
        frame.insert(name.to_string(), val);
    }

    eval.dynamic.push(frame);
    let result = eval.sf_progn(&tail[2..]);
    eval.dynamic.pop();
    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{format_eval_result, parse_forms, Evaluator};

    /// Helper: parse, evaluate all forms, return formatted results.
    fn eval_all(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    /// Helper: parse, evaluate all forms, return last result formatted.
    fn eval_last(src: &str) -> String {
        let results = eval_all(src);
        results.last().cloned().unwrap_or_else(|| "NONE".to_string())
    }

    /// Helper that provides an evaluator with cl-extra special forms wired up.
    /// Since we cannot modify eval.rs, we simulate the special forms by calling them directly.
    fn eval_with_cl(src: &str) -> String {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        let mut last = "NONE".to_string();
        for form in &forms {
            let result = eval_cl_form(&mut ev, form);
            last = format_eval_result(&result.map_err(super::super::error::map_flow));
        }
        last
    }

    fn eval_all_cl(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        forms.iter()
            .map(|form| {
                let result = eval_cl_form(&mut ev, form);
                format_eval_result(&result.map_err(super::super::error::map_flow))
            })
            .collect()
    }

    /// Evaluate a form, dispatching cl-* special forms manually.
    fn eval_cl_form(ev: &mut Evaluator, form: &Expr) -> EvalResult {
        if let Expr::List(items) = form {
            if let Some(Expr::Symbol(name)) = items.first() {
                let tail = &items[1..];
                match name.as_str() {
                    "cl-defstruct" => return sf_cl_defstruct(ev, tail),
                    "cl-loop" => return sf_cl_loop(ev, tail),
                    "cl-destructuring-bind" => return sf_cl_destructuring_bind(ev, tail),
                    "cl-incf" => return sf_cl_incf(ev, tail),
                    "cl-decf" => return sf_cl_decf(ev, tail),
                    "cl-push" => return sf_cl_push(ev, tail),
                    "cl-pop" => return sf_cl_pop(ev, tail),
                    "cl-pushnew" => return sf_cl_pushnew(ev, tail),
                    "cl-assert" => return sf_cl_assert(ev, tail),
                    "cl-check-type" => return sf_cl_check_type(ev, tail),
                    "cl-case" => return sf_cl_case(ev, tail),
                    "cl-ecase" => return sf_cl_ecase(ev, tail),
                    "cl-typecase" => return sf_cl_typecase(ev, tail),
                    "cl-etypecase" => return sf_cl_etypecase(ev, tail),
                    "cl-block" => return sf_cl_block(ev, tail),
                    "cl-return-from" => return sf_cl_return_from(ev, tail),
                    "cl-dotimes" => return sf_cl_dotimes(ev, tail),
                    "cl-dolist" => return sf_cl_dolist(ev, tail),
                    "cl-flet" => return sf_cl_flet(ev, tail),
                    "cl-labels" => return sf_cl_labels(ev, tail),
                    "cl-progv" => return sf_cl_progv(ev, tail),
                    _ => {}
                }
            }
        }
        ev.eval(form)
    }

    // ========================================================================
    // cl-defstruct tests
    // ========================================================================

    #[test]
    fn defstruct_basic_creation() {
        let results = eval_all_cl(
            "(cl-defstruct point x y)
             (make-point :x 10 :y 20)",
        );
        assert_eq!(results[0], "OK point");
        // Second result should be a vector
        assert!(results[1].starts_with("OK [cl-struct-point"));
    }

    #[test]
    fn defstruct_predicate() {
        let results = eval_all_cl(
            "(cl-defstruct point x y)
             (point-p (make-point :x 1 :y 2))
             (point-p 42)",
        );
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK nil");
    }

    #[test]
    fn defstruct_accessor() {
        let results = eval_all_cl(
            "(cl-defstruct point x y)
             (let ((p (make-point :x 10 :y 20)))
               (point-x p))",
        );
        assert_eq!(results[1], "OK 10");
    }

    #[test]
    fn defstruct_accessor_y() {
        let results = eval_all_cl(
            "(cl-defstruct point x y)
             (let ((p (make-point :x 10 :y 20)))
               (point-y p))",
        );
        assert_eq!(results[1], "OK 20");
    }

    #[test]
    fn defstruct_setter() {
        let results = eval_all_cl(
            "(cl-defstruct point x y)
             (let ((p (make-point :x 10 :y 20)))
               (setf-point-x p 99)
               (point-x p))",
        );
        assert_eq!(results[1], "OK 99");
    }

    #[test]
    fn defstruct_copier() {
        let results = eval_all_cl(
            "(cl-defstruct point x y)
             (let ((p (make-point :x 10 :y 20)))
               (let ((q (copy-point p)))
                 (setf-point-x q 99)
                 (list (point-x p) (point-x q))))",
        );
        assert_eq!(results[1], "OK (10 99)");
    }

    #[test]
    fn defstruct_default_values() {
        let results = eval_all_cl(
            "(cl-defstruct config (width 80) (height 24))
             (let ((c (make-config)))
               (list (config-width c) (config-height c)))",
        );
        assert_eq!(results[1], "OK (80 24)");
    }

    // ========================================================================
    // cl-loop tests
    // ========================================================================

    #[test]
    fn loop_for_from_to_collect() {
        let result = eval_with_cl(
            "(cl-loop for i from 1 to 5 collect i)",
        );
        assert_eq!(result, "OK (1 2 3 4 5)");
    }

    #[test]
    fn loop_for_from_below() {
        let result = eval_with_cl(
            "(cl-loop for i from 0 below 3 collect i)",
        );
        assert_eq!(result, "OK (0 1 2)");
    }

    #[test]
    fn loop_for_from_to_by() {
        let result = eval_with_cl(
            "(cl-loop for i from 0 to 10 by 3 collect i)",
        );
        assert_eq!(result, "OK (0 3 6 9)");
    }

    #[test]
    fn loop_for_in_list() {
        let result = eval_with_cl(
            "(cl-loop for x in '(a b c) collect x)",
        );
        assert_eq!(result, "OK (a b c)");
    }

    #[test]
    fn loop_for_across_vector() {
        let result = eval_with_cl(
            "(cl-loop for x across [10 20 30] collect x)",
        );
        assert_eq!(result, "OK (10 20 30)");
    }

    #[test]
    fn loop_for_equals_then() {
        let result = eval_with_cl(
            "(cl-loop for x = 1 then (* x 2) repeat 5 collect x)",
        );
        assert_eq!(result, "OK (1 2 4 8 16)");
    }

    #[test]
    fn loop_sum() {
        let result = eval_with_cl(
            "(cl-loop for i from 1 to 5 sum i)",
        );
        assert_eq!(result, "OK 15");
    }

    #[test]
    fn loop_count() {
        let result = eval_with_cl(
            "(cl-loop for i from 1 to 10 count (= 0 (% i 2)))",
        );
        assert_eq!(result, "OK 5");
    }

    #[test]
    fn loop_maximize() {
        let result = eval_with_cl(
            "(cl-loop for x in '(3 1 4 1 5 9) maximize x)",
        );
        assert_eq!(result, "OK 9");
    }

    #[test]
    fn loop_minimize() {
        let result = eval_with_cl(
            "(cl-loop for x in '(3 1 4 1 5 9) minimize x)",
        );
        assert_eq!(result, "OK 1");
    }

    #[test]
    fn loop_while_condition() {
        let result = eval_with_cl(
            "(cl-loop for i from 0 while (< i 3) collect i)",
        );
        assert_eq!(result, "OK (0 1 2)");
    }

    #[test]
    fn loop_until_condition() {
        let result = eval_with_cl(
            "(cl-loop for i from 0 until (= i 3) collect i)",
        );
        assert_eq!(result, "OK (0 1 2)");
    }

    #[test]
    fn loop_repeat() {
        let result = eval_with_cl(
            "(cl-loop repeat 4 collect 42)",
        );
        assert_eq!(result, "OK (42 42 42 42)");
    }

    #[test]
    fn loop_do_side_effects() {
        let results = eval_all_cl(
            "(defvar total 0)
             (cl-loop for i from 1 to 3 do (setq total (+ total i)))
             total",
        );
        assert_eq!(results[2], "OK 6");
    }

    #[test]
    fn loop_with_variable() {
        let result = eval_with_cl(
            "(cl-loop with offset = 100
                      for i from 1 to 3
                      collect (+ i offset))",
        );
        assert_eq!(result, "OK (101 102 103)");
    }

    #[test]
    fn loop_return_early() {
        let result = eval_with_cl(
            "(cl-loop for i from 1 to 100
                      do (when (= i 5) nil)
                      return i)",
        );
        // The return clause triggers on first iteration
        assert_eq!(result, "OK 1");
    }

    #[test]
    fn loop_append() {
        let result = eval_with_cl(
            "(cl-loop for x in '(1 2 3) append (list x (* x 10)))",
        );
        assert_eq!(result, "OK (1 10 2 20 3 30)");
    }

    // ========================================================================
    // cl-destructuring-bind tests
    // ========================================================================

    #[test]
    fn destructure_simple_list() {
        let result = eval_with_cl(
            "(cl-destructuring-bind (a b c) '(1 2 3)
               (list a b c))",
        );
        assert_eq!(result, "OK (1 2 3)");
    }

    #[test]
    fn destructure_nested_list() {
        let result = eval_with_cl(
            "(cl-destructuring-bind (a (b1 b2) c) '(1 (2 3) 4)
               (list a b1 b2 c))",
        );
        assert_eq!(result, "OK (1 2 3 4)");
    }

    #[test]
    fn destructure_with_rest() {
        let result = eval_with_cl(
            "(cl-destructuring-bind (a &rest b) '(1 2 3 4)
               (list a b))",
        );
        assert_eq!(result, "OK (1 (2 3 4))");
    }

    #[test]
    fn destructure_with_optional() {
        let result = eval_with_cl(
            "(cl-destructuring-bind (a &optional b c) '(1)
               (list a b c))",
        );
        assert_eq!(result, "OK (1 nil nil)");
    }

    // ========================================================================
    // cl-incf / cl-decf tests
    // ========================================================================

    #[test]
    fn cl_incf_basic() {
        let results = eval_all_cl(
            "(defvar x 10)
             (cl-incf x)
             x",
        );
        assert_eq!(results[2], "OK 11");
    }

    #[test]
    fn cl_incf_with_delta() {
        let results = eval_all_cl(
            "(defvar x 10)
             (cl-incf x 5)
             x",
        );
        assert_eq!(results[2], "OK 15");
    }

    #[test]
    fn cl_decf_basic() {
        let results = eval_all_cl(
            "(defvar x 10)
             (cl-decf x)
             x",
        );
        assert_eq!(results[2], "OK 9");
    }

    // ========================================================================
    // cl-push / cl-pop / cl-pushnew tests
    // ========================================================================

    #[test]
    fn cl_push_basic() {
        let results = eval_all_cl(
            "(defvar lst '(2 3))
             (cl-push 1 lst)
             lst",
        );
        assert_eq!(results[2], "OK (1 2 3)");
    }

    #[test]
    fn cl_pop_basic() {
        let results = eval_all_cl(
            "(defvar lst '(1 2 3))
             (cl-pop lst)",
        );
        // cl-pop returns the popped element
        assert_eq!(results[1], "OK 1");
    }

    #[test]
    fn cl_pop_modifies_list() {
        let results = eval_all_cl(
            "(defvar lst '(1 2 3))
             (cl-pop lst)
             lst",
        );
        assert_eq!(results[2], "OK (2 3)");
    }

    #[test]
    fn cl_pushnew_already_present() {
        let results = eval_all_cl(
            "(defvar lst '(1 2 3))
             (cl-pushnew 2 lst)
             lst",
        );
        assert_eq!(results[2], "OK (1 2 3)");
    }

    #[test]
    fn cl_pushnew_new_item() {
        let results = eval_all_cl(
            "(defvar lst '(1 2 3))
             (cl-pushnew 4 lst)
             lst",
        );
        assert_eq!(results[2], "OK (4 1 2 3)");
    }

    // ========================================================================
    // cl-assert / cl-check-type tests
    // ========================================================================

    #[test]
    fn cl_assert_passes() {
        let result = eval_with_cl("(cl-assert t)");
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn cl_assert_fails() {
        let result = eval_with_cl("(cl-assert nil)");
        assert!(result.contains("cl-assertion-failed"));
    }

    #[test]
    fn cl_check_type_passes() {
        let result = eval_with_cl("(cl-check-type 42 integer)");
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn cl_check_type_fails() {
        let result = eval_with_cl("(cl-check-type \"hello\" integer)");
        assert!(result.contains("wrong-type-argument"));
    }

    // ========================================================================
    // cl-case / cl-ecase tests
    // ========================================================================

    #[test]
    fn cl_case_match_symbol() {
        let result = eval_with_cl(
            "(cl-case 'b
               (a 1)
               (b 2)
               (c 3))",
        );
        assert_eq!(result, "OK 2");
    }

    #[test]
    fn cl_case_match_int() {
        let result = eval_with_cl(
            "(cl-case 2
               (1 'one)
               (2 'two)
               (3 'three))",
        );
        assert_eq!(result, "OK two");
    }

    #[test]
    fn cl_case_otherwise() {
        let result = eval_with_cl(
            "(cl-case 99
               (1 'one)
               (otherwise 'other))",
        );
        assert_eq!(result, "OK other");
    }

    #[test]
    fn cl_case_list_keys() {
        let result = eval_with_cl(
            "(cl-case 3
               ((1 2) 'low)
               ((3 4) 'mid)
               ((5 6) 'high))",
        );
        assert_eq!(result, "OK mid");
    }

    #[test]
    fn cl_ecase_no_match_signals_error() {
        let result = eval_with_cl(
            "(cl-ecase 99
               (1 'one)
               (2 'two))",
        );
        assert!(result.contains("cl-ecase-error"));
    }

    // ========================================================================
    // cl-typecase / cl-etypecase tests
    // ========================================================================

    #[test]
    fn cl_typecase_integer() {
        let result = eval_with_cl(
            "(cl-typecase 42
               (string 'str)
               (integer 'int)
               (float 'flt))",
        );
        assert_eq!(result, "OK int");
    }

    #[test]
    fn cl_typecase_string() {
        let result = eval_with_cl(
            "(cl-typecase \"hello\"
               (string 'str)
               (integer 'int))",
        );
        assert_eq!(result, "OK str");
    }

    #[test]
    fn cl_etypecase_no_match() {
        let result = eval_with_cl(
            "(cl-etypecase '(1 2)
               (string 'str)
               (integer 'int))",
        );
        assert!(result.contains("cl-etypecase-error"));
    }

    // ========================================================================
    // cl-block / cl-return-from tests
    // ========================================================================

    #[test]
    fn cl_block_normal() {
        let result = eval_with_cl(
            "(cl-block myblock 1 2 3)",
        );
        assert_eq!(result, "OK 3");
    }

    #[test]
    fn cl_block_return_from() {
        // Since cl-return-from is a special form that must be dispatched by our handler,
        // and it's nested inside cl-block's body where the standard evaluator runs,
        // we test it by directly calling the Rust functions.
        let forms = parse_forms("(cl-block myblock (throw '--cl-block-myblock 42) 99)").expect("parse");
        let mut ev = Evaluator::new();
        let result = eval_cl_form(&mut ev, &forms[0]);
        let formatted = format_eval_result(&result.map_err(super::super::error::map_flow));
        assert_eq!(formatted, "OK 42");
    }

    // ========================================================================
    // cl-dotimes / cl-dolist tests
    // ========================================================================

    #[test]
    fn cl_dotimes_basic() {
        let results = eval_all_cl(
            "(defvar sum 0)
             (cl-dotimes (i 5) (setq sum (+ sum i)))
             sum",
        );
        assert_eq!(results[2], "OK 10");
    }

    #[test]
    fn cl_dolist_basic() {
        let results = eval_all_cl(
            "(defvar result nil)
             (cl-dolist (x '(a b c)) (setq result (cons x result)))
             result",
        );
        assert_eq!(results[2], "OK (c b a)");
    }

    // ========================================================================
    // cl-flet / cl-labels tests
    // ========================================================================

    #[test]
    fn cl_flet_local_function() {
        let result = eval_with_cl(
            "(cl-flet ((double (x) (* x 2)))
               (double 21))",
        );
        assert_eq!(result, "OK 42");
    }

    #[test]
    fn cl_flet_does_not_leak() {
        let results = eval_all_cl(
            "(defun add1 (x) (+ x 1))
             (cl-flet ((add1 (x) (+ x 100)))
               (add1 5))
             (add1 5)",
        );
        assert_eq!(results[1], "OK 105");
        assert_eq!(results[2], "OK 6");
    }

    #[test]
    fn cl_labels_recursive() {
        let result = eval_with_cl(
            "(cl-labels ((fact (n) (if (<= n 1) 1 (* n (fact (- n 1))))))
               (fact 5))",
        );
        assert_eq!(result, "OK 120");
    }

    // ========================================================================
    // cl-progv tests
    // ========================================================================

    #[test]
    fn cl_progv_basic() {
        let result = eval_with_cl(
            "(cl-progv '(a b) '(10 20)
               (+ a b))",
        );
        assert_eq!(result, "OK 30");
    }

    #[test]
    fn cl_progv_restores() {
        let results = eval_all_cl(
            "(defvar myvar 1)
             (cl-progv '(myvar) '(99)
               myvar)
             myvar",
        );
        assert_eq!(results[1], "OK 99");
        assert_eq!(results[2], "OK 1");
    }

    // ========================================================================
    // Additional mixed / edge-case tests
    // ========================================================================

    #[test]
    fn loop_empty_body() {
        let result = eval_with_cl("(cl-loop)");
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn loop_downfrom() {
        let result = eval_with_cl(
            "(cl-loop for i downfrom 5 to 1 collect i)",
        );
        assert_eq!(result, "OK (5 4 3 2 1)");
    }

    #[test]
    fn destructure_discard_with_underscore() {
        let result = eval_with_cl(
            "(cl-destructuring-bind (_ b _) '(1 2 3) b)",
        );
        assert_eq!(result, "OK 2");
    }
}
