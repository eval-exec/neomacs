//! Elisp interpreter module.
//!
//! Provides a full Elisp evaluator with:
//! - Value types: nil, t, int, float, string, symbol, keyword, char, cons, vector, hash-table
//! - Complete parser: strings, floats, chars, vectors, dotted pairs, quasiquote, reader macros
//! - Special forms: quote, function, let, let*, setq, if, and, or, cond, while, progn, prog1,
//!   lambda, defun, defvar, defconst, defmacro, funcall, catch, throw, unwind-protect,
//!   condition-case, when, unless
//! - 100+ built-in functions: arithmetic, comparisons, type predicates, list ops, string ops,
//!   vector ops, hash tables, higher-order functions, conversion, property lists

pub mod value;
pub mod expr;
pub mod parser;
pub mod error;
pub mod eval;
pub mod print;
pub mod builtins;
pub mod symbol;
pub mod load;
pub mod bytecode;
pub mod keymap;
pub mod regex;
pub mod fileio;
pub mod syntax;
pub mod advice;
pub mod process;
pub mod network;
pub mod timer;
pub mod builtins_extra;
pub mod isearch;
pub mod debug;
pub mod minibuffer;
pub mod mode;
pub mod register;
pub mod bookmark;
pub mod abbrev;
pub mod cl_lib;
pub mod autoload;
pub mod custom;
pub mod kill_ring;
pub mod interactive;
pub mod window_cmds;
pub mod textprop;
pub mod navigation;
pub mod chartable;
pub mod errors;
pub mod pcase;
pub mod setf;
pub mod cl_extra;

// Re-export the main public API
pub use value::{Value, ConsCell, LambdaData, LambdaParams};
pub use expr::{Expr, ParseError, print_expr};
pub use parser::parse_forms;
pub use error::{EvalError, format_eval_result};
pub use eval::Evaluator;
pub use print::print_value;
pub use symbol::Obarray;
pub use bytecode::{ByteCodeFunction, Compiler as ByteCompiler, Vm as ByteVm};

/// Convenience: parse and evaluate source code.
pub fn eval_source(input: &str) -> Result<Vec<Result<Value, EvalError>>, ParseError> {
    let forms = parse_forms(input)?;
    let mut evaluator = Evaluator::new();
    Ok(evaluator.eval_forms(&forms))
}
