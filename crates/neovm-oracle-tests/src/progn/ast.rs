//! Random AST generator for progn-focused oracle parity tests.

use proptest::prelude::*;

#[derive(Clone, Debug)]
enum PgSym {
    X,
    Y,
    Z,
}

impl PgSym {
    fn as_str(&self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

#[derive(Clone, Debug)]
enum PgTag {
    A,
    B,
    C,
}

impl PgTag {
    fn as_quoted_symbol(&self) -> &'static str {
        match self {
            Self::A => "'neovm--pg-tag-a",
            Self::B => "'neovm--pg-tag-b",
            Self::C => "'neovm--pg-tag-c",
        }
    }
}

#[derive(Clone, Debug)]
enum PgExpr {
    Int(i64),
    Sym(PgSym),
    Nil,
    Progn(Vec<PgExpr>),
    If(Box<PgExpr>, Box<PgExpr>, Box<PgExpr>),
    Let(PgSym, Box<PgExpr>, Box<PgExpr>),
    Setq(PgSym, Box<PgExpr>),
    Add(Box<PgExpr>, Box<PgExpr>),
    Sub(Box<PgExpr>, Box<PgExpr>),
    List(Vec<PgExpr>),
    Catch(PgTag, Box<PgExpr>),
    Throw(PgTag, Box<PgExpr>),
    UnwindProtect(Box<PgExpr>, Box<PgExpr>),
    Lambda0(Box<PgExpr>),
    Funcall0(Box<PgExpr>),
    FuncallPrognLambda {
        seed: Box<PgExpr>,
        delta: Box<PgExpr>,
        arg: Box<PgExpr>,
    },
}

impl PgExpr {
    fn to_elisp(&self) -> String {
        match self {
            Self::Int(n) => n.to_string(),
            Self::Sym(sym) => sym.as_str().to_string(),
            Self::Nil => "nil".to_string(),
            Self::Progn(forms) => {
                if forms.is_empty() {
                    "(progn)".to_string()
                } else {
                    format!(
                        "(progn {})",
                        forms
                            .iter()
                            .map(Self::to_elisp)
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                }
            }
            Self::If(cond, then_expr, else_expr) => {
                format!(
                    "(if {} {} {})",
                    cond.to_elisp(),
                    then_expr.to_elisp(),
                    else_expr.to_elisp()
                )
            }
            Self::Let(sym, value, body) => {
                format!(
                    "(let (({} {})) {})",
                    sym.as_str(),
                    value.to_elisp(),
                    body.to_elisp()
                )
            }
            Self::Setq(sym, value) => format!("(setq {} {})", sym.as_str(), value.to_elisp()),
            Self::Add(lhs, rhs) => format!("(+ {} {})", lhs.to_elisp(), rhs.to_elisp()),
            Self::Sub(lhs, rhs) => format!("(- {} {})", lhs.to_elisp(), rhs.to_elisp()),
            Self::List(items) => {
                if items.is_empty() {
                    "(list)".to_string()
                } else {
                    format!(
                        "(list {})",
                        items
                            .iter()
                            .map(Self::to_elisp)
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                }
            }
            Self::Catch(tag, body) => {
                format!("(catch {} {})", tag.as_quoted_symbol(), body.to_elisp())
            }
            Self::Throw(tag, value) => {
                format!("(throw {} {})", tag.as_quoted_symbol(), value.to_elisp())
            }
            Self::UnwindProtect(body, cleanup) => {
                format!(
                    "(unwind-protect {} {})",
                    body.to_elisp(),
                    cleanup.to_elisp()
                )
            }
            Self::Lambda0(body) => format!("(lambda () {})", body.to_elisp()),
            Self::Funcall0(fun) => format!("(funcall {})", fun.to_elisp()),
            Self::FuncallPrognLambda { seed, delta, arg } => format!(
                "(let ((x {})) (funcall (progn (setq x {}) (lambda (a) (+ x a))) {}))",
                seed.to_elisp(),
                delta.to_elisp(),
                arg.to_elisp()
            ),
        }
    }
}

fn arb_sym() -> impl Strategy<Value = PgSym> {
    prop_oneof![Just(PgSym::X), Just(PgSym::Y), Just(PgSym::Z),]
}

fn arb_tag() -> impl Strategy<Value = PgTag> {
    prop_oneof![Just(PgTag::A), Just(PgTag::B), Just(PgTag::C),]
}

fn arb_progn_expr() -> BoxedStrategy<PgExpr> {
    let leaf = prop_oneof![
        (-100i64..100i64).prop_map(PgExpr::Int),
        arb_sym().prop_map(PgExpr::Sym),
        Just(PgExpr::Nil),
    ];

    leaf.prop_recursive(5, 64, 8, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(PgExpr::Progn),
            (inner.clone(), inner.clone(), inner.clone()).prop_map(|(c, t, e)| PgExpr::If(
                Box::new(c),
                Box::new(t),
                Box::new(e)
            )),
            (arb_sym(), inner.clone(), inner.clone()).prop_map(|(s, v, b)| PgExpr::Let(
                s,
                Box::new(v),
                Box::new(b)
            )),
            (arb_sym(), inner.clone()).prop_map(|(s, v)| PgExpr::Setq(s, Box::new(v))),
            (inner.clone(), inner.clone()).prop_map(|(l, r)| PgExpr::Add(Box::new(l), Box::new(r))),
            (inner.clone(), inner.clone()).prop_map(|(l, r)| PgExpr::Sub(Box::new(l), Box::new(r))),
            proptest::collection::vec(inner.clone(), 0..4).prop_map(PgExpr::List),
            (arb_tag(), inner.clone()).prop_map(|(t, b)| PgExpr::Catch(t, Box::new(b))),
            (arb_tag(), inner.clone()).prop_map(|(t, v)| PgExpr::Throw(t, Box::new(v))),
            (inner.clone(), inner.clone())
                .prop_map(|(b, c)| PgExpr::UnwindProtect(Box::new(b), Box::new(c))),
            inner
                .clone()
                .prop_map(|body| PgExpr::Lambda0(Box::new(body))),
            inner
                .clone()
                .prop_map(|fun| PgExpr::Funcall0(Box::new(fun))),
            (inner.clone(), inner.clone(), inner.clone()).prop_map(|(seed, delta, arg)| {
                PgExpr::FuncallPrognLambda {
                    seed: Box::new(seed),
                    delta: Box::new(delta),
                    arg: Box::new(arg),
                }
            }),
        ]
    })
    .boxed()
}

fn wrap_with_lexical_bindings(expr: &PgExpr) -> String {
    format!("(let ((x 0) (y 1) (z 2)) {})", expr.to_elisp())
}

pub(crate) fn arb_progn_form() -> BoxedStrategy<String> {
    arb_progn_expr()
        .prop_map(|expr| wrap_with_lexical_bindings(&expr))
        .boxed()
}
