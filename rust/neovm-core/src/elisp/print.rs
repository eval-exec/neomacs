//! Value printing (Lisp representation).

use super::expr::{self, Expr};
use super::value::Value;

/// Print a `Value` as a Lisp string.
pub fn print_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::True => "t".to_string(),
        Value::Int(v) => v.to_string(),
        Value::Float(f) => format_float(*f),
        Value::Symbol(s) => s.clone(),
        Value::Keyword(s) => s.clone(),
        Value::Str(s) => format!("{:?}", **s),
        Value::Char(c) => format!("?{}", format_char(*c)),
        Value::Cons(_) => {
            let mut out = String::from("(");
            print_cons(value, &mut out);
            out.push(')');
            out
        }
        Value::Vector(v) => {
            let items = v.lock().expect("poisoned");
            let parts: Vec<String> = items.iter().map(print_value).collect();
            format!("[{}]", parts.join(" "))
        }
        Value::HashTable(_) => "#<hash-table>".to_string(),
        Value::Lambda(lambda) => {
            let params = format_params(&lambda.params);
            let body = lambda
                .body
                .iter()
                .map(|e| expr::print_expr(e))
                .collect::<Vec<_>>()
                .join(" ");
            if lambda.env.is_some() {
                format!("(closure {} {})", params, body)
            } else {
                format!("(lambda {} {})", params, body)
            }
        }
        Value::Macro(m) => {
            let params = format_params(&m.params);
            let body = m
                .body
                .iter()
                .map(|e| expr::print_expr(e))
                .collect::<Vec<_>>()
                .join(" ");
            format!("(macro {} {})", params, body)
        }
        Value::Subr(name) => format!("#<subr {}>", name),
        Value::ByteCode(bc) => {
            let params = format_params(&bc.params);
            format!("#<bytecode {} ({} ops)>", params, bc.ops.len())
        }
        Value::Buffer(id) => format!("#<buffer {}>", id.0),
        Value::Timer(id) => format!("#<timer {}>", id),
    }
}

/// Re-export for compatibility.
pub fn print_expr(expr: &Expr) -> String {
    expr::print_expr(expr)
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "0.0e+NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "1.0e+INF".to_string()
        } else {
            "-1.0e+INF".to_string()
        };
    }
    if f.fract() == 0.0 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

fn format_char(c: char) -> String {
    match c {
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\\' => "\\\\".to_string(),
        ' ' => "\\s".to_string(),
        _ => c.to_string(),
    }
}

fn format_params(params: &super::value::LambdaParams) -> String {
    let mut parts = Vec::new();
    for p in &params.required {
        parts.push(p.clone());
    }
    if !params.optional.is_empty() {
        parts.push("&optional".to_string());
        for p in &params.optional {
            parts.push(p.clone());
        }
    }
    if let Some(ref rest) = params.rest {
        parts.push("&rest".to_string());
        parts.push(rest.clone());
    }
    if parts.is_empty() {
        "nil".to_string()
    } else {
        format!("({})", parts.join(" "))
    }
}

fn print_cons(value: &Value, out: &mut String) {
    let mut cursor = value.clone();
    let mut first = true;
    loop {
        match cursor {
            Value::Cons(cell) => {
                if !first {
                    out.push(' ');
                }
                let pair = cell.lock().expect("poisoned");
                out.push_str(&print_value(&pair.car));
                cursor = pair.cdr.clone();
                first = false;
            }
            Value::Nil => return,
            other => {
                if !first {
                    out.push_str(" . ");
                }
                out.push_str(&print_value(&other));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::value::{LambdaData, LambdaParams};
    use std::sync::Arc;

    #[test]
    fn print_basic_values() {
        assert_eq!(print_value(&Value::Nil), "nil");
        assert_eq!(print_value(&Value::True), "t");
        assert_eq!(print_value(&Value::Int(42)), "42");
        assert_eq!(print_value(&Value::Float(3.14)), "3.14");
        assert_eq!(print_value(&Value::Float(1.0)), "1.0");
        assert_eq!(print_value(&Value::symbol("foo")), "foo");
        assert_eq!(print_value(&Value::keyword(":bar")), ":bar");
    }

    #[test]
    fn print_string() {
        assert_eq!(print_value(&Value::string("hello")), "\"hello\"");
    }

    #[test]
    fn print_list() {
        let lst = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(print_value(&lst), "(1 2 3)");
    }

    #[test]
    fn print_dotted_pair() {
        let pair = Value::cons(Value::Int(1), Value::Int(2));
        assert_eq!(print_value(&pair), "(1 . 2)");
    }

    #[test]
    fn print_vector() {
        let v = Value::vector(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(print_value(&v), "[1 2]");
    }

    #[test]
    fn print_lambda() {
        let lam = Value::Lambda(Arc::new(LambdaData {
            params: LambdaParams::simple(vec!["x".into(), "y".into()]),
            body: vec![Expr::List(vec![
                Expr::Symbol("+".into()),
                Expr::Symbol("x".into()),
                Expr::Symbol("y".into()),
            ])],
            env: None,
            docstring: None,
        }));
        assert_eq!(print_value(&lam), "(lambda (x y) (+ x y))");
    }
}
