use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Symbol(String),
    List(Vec<Expr>),
}

#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Int(i64),
    Symbol(String),
    Cons(Arc<Mutex<ConsCell>>),
    Lambda(Arc<Lambda>),
}

#[derive(Clone, Debug)]
pub struct ConsCell {
    pub car: Value,
    pub cdr: Value,
}

#[derive(Clone, Debug)]
pub struct Lambda {
    pub params: Vec<String>,
    pub body: Vec<Expr>,
}

#[derive(Clone, Debug)]
pub struct ParseError {
    pub position: usize,
    pub message: String,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at {}: {}", self.position, self.message)
    }
}

impl Error for ParseError {}

#[derive(Clone, Debug)]
pub enum EvalError {
    Signal { symbol: String, data: Vec<Value> },
    UncaughtThrow { tag: Value, value: Value },
}

impl Display for EvalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal { symbol, data } => {
                write!(f, "signal {} {}", symbol, print_value(&list_from_vec(data.clone())))
            }
            Self::UncaughtThrow { tag, value } => write!(
                f,
                "uncaught throw tag={} value={}",
                print_value(tag),
                print_value(value)
            ),
        }
    }
}

impl Error for EvalError {}

#[derive(Clone, Debug)]
struct SignalData {
    symbol: String,
    data: Vec<Value>,
}

#[derive(Clone, Debug)]
enum Flow {
    Signal(SignalData),
    Throw { tag: Value, value: Value },
}

type EvalResult = Result<Value, Flow>;

pub struct Evaluator {
    globals: HashMap<String, Value>,
    functions: HashMap<String, Value>,
    dynamic: Vec<HashMap<String, Value>>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert("t".to_string(), Value::Symbol("t".to_string()));
        let mut functions = HashMap::new();
        functions.insert("+".to_string(), Value::Symbol("+".to_string()));
        functions.insert("/".to_string(), Value::Symbol("/".to_string()));
        functions.insert("setcar".to_string(), Value::Symbol("setcar".to_string()));
        functions.insert("setcdr".to_string(), Value::Symbol("setcdr".to_string()));
        functions.insert("car".to_string(), Value::Symbol("car".to_string()));
        functions.insert("cdr".to_string(), Value::Symbol("cdr".to_string()));
        functions.insert("cons".to_string(), Value::Symbol("cons".to_string()));
        functions.insert("list".to_string(), Value::Symbol("list".to_string()));
        functions.insert("eq".to_string(), Value::Symbol("eq".to_string()));
        functions.insert("equal".to_string(), Value::Symbol("equal".to_string()));
        Self {
            globals,
            functions,
            dynamic: Vec::new(),
        }
    }

    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, EvalError> {
        self.eval(expr).map_err(map_flow)
    }

    pub fn eval_forms(&mut self, forms: &[Expr]) -> Vec<Result<Value, EvalError>> {
        forms.iter().map(|form| self.eval_expr(form)).collect()
    }

    fn eval(&mut self, expr: &Expr) -> EvalResult {
        match expr {
            Expr::Int(v) => Ok(Value::Int(*v)),
            Expr::Symbol(symbol) => self.eval_symbol(symbol),
            Expr::List(items) => self.eval_list(items),
        }
    }

    fn eval_symbol(&self, symbol: &str) -> EvalResult {
        if symbol == "nil" {
            return Ok(Value::Nil);
        }
        if symbol == "t" {
            return Ok(Value::Symbol("t".to_string()));
        }

        for frame in self.dynamic.iter().rev() {
            if let Some(value) = frame.get(symbol) {
                return Ok(value.clone());
            }
        }

        if let Some(value) = self.globals.get(symbol) {
            return Ok(value.clone());
        }

        Err(signal(
            "void-variable",
            vec![Value::Symbol(symbol.to_string())],
        ))
    }

    fn eval_list(&mut self, items: &[Expr]) -> EvalResult {
        let Some((head, tail)) = items.split_first() else {
            return Ok(Value::Nil);
        };

        if let Expr::Symbol(name) = head {
            match name.as_str() {
                "quote" => return self.eval_quote(tail),
                "let" => return self.eval_let(tail),
                "let*" => return self.eval_let_star(tail),
                "setq" => return self.eval_setq(tail),
                "catch" => return self.eval_catch(tail),
                "throw" => return self.eval_throw(tail),
                "condition-case" => return self.eval_condition_case(tail),
                "if" => return self.eval_if(tail),
                "progn" => return self.eval_progn(tail),
                "lambda" => return self.eval_lambda(tail),
                "defun" => return self.eval_defun(tail),
                "funcall" => return self.eval_funcall(tail),
                "+" => return self.eval_plus(tail),
                "/" => return self.eval_div(tail),
                "setcar" => return self.eval_setcar(tail),
                "setcdr" => return self.eval_setcdr(tail),
                "car" => return self.eval_car(tail),
                "cdr" => return self.eval_cdr(tail),
                "cons" => return self.eval_cons(tail),
                "list" => return self.eval_list_builtin(tail),
                "eq" => return self.eval_eq(tail),
                "equal" => return self.eval_equal(tail),
                _ => {}
            }

            let mut args = Vec::with_capacity(tail.len());
            for expr in tail {
                args.push(self.eval(expr)?);
            }
            return self.apply_named(name, args);
        }

        Err(signal(
            "void-function",
            vec![quote_to_value(head)],
        ))
    }

    fn eval_quote(&self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        Ok(quote_to_value(&tail[0]))
    }

    fn eval_let(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let bindings_expr = &tail[0];
        let body = &tail[1..];

        let mut bindings = HashMap::new();
        match bindings_expr {
            Expr::List(entries) => {
                for binding in entries {
                    match binding {
                        Expr::Symbol(name) => {
                            bindings.insert(name.clone(), Value::Nil);
                        }
                        Expr::List(pair) if pair.len() == 2 => {
                            let Expr::Symbol(name) = &pair[0] else {
                                return Err(signal("wrong-type-argument", vec![]));
                            };
                            let value = self.eval(&pair[1])?;
                            bindings.insert(name.clone(), value);
                        }
                        _ => return Err(signal("wrong-type-argument", vec![])),
                    }
                }
            }
            _ => return Err(signal("wrong-type-argument", vec![])),
        }

        self.dynamic.push(bindings);
        let result = self.eval_progn(body);
        self.dynamic.pop();
        result
    }

    fn eval_setq(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() || !tail.len().is_multiple_of(2) {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let mut last = Value::Nil;
        let mut i = 0;
        while i < tail.len() {
            let Expr::Symbol(name) = &tail[i] else {
                return Err(signal("wrong-type-argument", vec![]));
            };
            let value = self.eval(&tail[i + 1])?;
            self.assign(name, value.clone());
            last = value;
            i += 2;
        }

        Ok(last)
    }

    fn eval_let_star(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let Expr::List(entries) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };

        self.dynamic.push(HashMap::new());
        for binding in entries {
            match binding {
                Expr::Symbol(name) => self.assign(name, Value::Nil),
                Expr::List(pair) if pair.len() == 2 => {
                    let Expr::Symbol(name) = &pair[0] else {
                        self.dynamic.pop();
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    let value = self.eval(&pair[1])?;
                    self.assign(name, value);
                }
                _ => {
                    self.dynamic.pop();
                    return Err(signal("wrong-type-argument", vec![]));
                }
            }
        }

        let result = self.eval_progn(&tail[1..]);
        self.dynamic.pop();
        result
    }

    fn eval_catch(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let tag = self.eval(&tail[0])?;
        match self.eval_progn(&tail[1..]) {
            Ok(value) => Ok(value),
            Err(Flow::Throw {
                tag: thrown_tag,
                value,
            }) if eq_value(&tag, &thrown_tag) => Ok(value),
            Err(flow) => Err(flow),
        }
    }

    fn eval_throw(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let tag = self.eval(&tail[0])?;
        let value = self.eval(&tail[1])?;
        Err(Flow::Throw { tag, value })
    }

    fn eval_condition_case(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 3 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let var = match &tail[0] {
            Expr::Symbol(name) => name.clone(),
            _ => return Err(signal("wrong-type-argument", vec![])),
        };
        let body = &tail[1];
        let handlers = &tail[2..];

        match self.eval(body) {
            Ok(value) => Ok(value),
            Err(Flow::Signal(sig)) => {
                for handler in handlers {
                    let Expr::List(handler_items) = handler else {
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    if handler_items.is_empty() {
                        return Err(signal("wrong-type-argument", vec![]));
                    }

                    if signal_matches(&handler_items[0], &sig.symbol) {
                        let mut frame = HashMap::new();
                        if var != "nil" {
                            frame.insert(var.clone(), make_signal_binding_value(&sig));
                        }
                        self.dynamic.push(frame);
                        let result = self.eval_progn(&handler_items[1..]);
                        self.dynamic.pop();
                        return result;
                    }
                }

                Err(Flow::Signal(sig))
            }
            Err(flow) => Err(flow),
        }
    }

    fn eval_if(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let cond = self.eval(&tail[0])?;
        if is_truthy(&cond) {
            self.eval(&tail[1])
        } else {
            self.eval_progn(&tail[2..])
        }
    }

    fn eval_lambda(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let params_expr = &tail[0];
        let mut params = Vec::new();
        match params_expr {
            Expr::Symbol(symbol) if symbol == "nil" => {}
            Expr::List(items) => {
                for item in items {
                    let Expr::Symbol(symbol) = item else {
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    params.push(symbol.clone());
                }
            }
            _ => return Err(signal("wrong-type-argument", vec![])),
        }

        Ok(Value::Lambda(Arc::new(Lambda {
            params,
            body: tail[1..].to_vec(),
        })))
    }

    fn eval_defun(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 3 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };

        let lambda = self.eval_lambda(&tail[1..])?;
        self.functions.insert(name.clone(), lambda);
        Ok(Value::Symbol(name.clone()))
    }

    fn eval_funcall(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let function = self.eval(&tail[0])?;
        let mut args = Vec::with_capacity(tail.len().saturating_sub(1));
        for expr in &tail[1..] {
            args.push(self.eval(expr)?);
        }
        self.apply(function, args)
    }

    fn eval_plus(&mut self, tail: &[Expr]) -> EvalResult {
        let mut total = 0i64;
        for expr in tail {
            total = total
                .checked_add(expect_int(self.eval(expr)?)?)
                .ok_or_else(|| signal("overflow-error", vec![]))?;
        }
        Ok(Value::Int(total))
    }

    fn eval_div(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let mut iter = tail.iter();
        let first = iter.next().expect("division requires first arg");
        let mut acc = expect_int(self.eval(first)?)?;
        for expr in iter {
            let divisor = expect_int(self.eval(expr)?)?;
            if divisor == 0 {
                return Err(signal("arith-error", vec![]));
            }
            acc /= divisor;
        }

        Ok(Value::Int(acc))
    }

    fn eval_setcar(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let list = self.eval(&tail[0])?;
        let value = self.eval(&tail[1])?;

        match list {
            Value::Cons(cell) => {
                let mut pair = cell.lock().expect("cons cell mutex poisoned");
                pair.car = value.clone();
                Ok(value)
            }
            _ => Err(signal("wrong-type-argument", vec![])),
        }
    }

    fn eval_setcdr(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let list = self.eval(&tail[0])?;
        let value = self.eval(&tail[1])?;

        match list {
            Value::Cons(cell) => {
                let mut pair = cell.lock().expect("cons cell mutex poisoned");
                pair.cdr = value.clone();
                Ok(value)
            }
            _ => Err(signal("wrong-type-argument", vec![])),
        }
    }

    fn eval_car(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let list = self.eval(&tail[0])?;
        match list {
            Value::Nil => Ok(Value::Nil),
            Value::Cons(cell) => Ok(cell.lock().expect("cons cell mutex poisoned").car.clone()),
            _ => Err(signal("wrong-type-argument", vec![])),
        }
    }

    fn eval_cdr(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let list = self.eval(&tail[0])?;
        match list {
            Value::Nil => Ok(Value::Nil),
            Value::Cons(cell) => Ok(cell.lock().expect("cons cell mutex poisoned").cdr.clone()),
            _ => Err(signal("wrong-type-argument", vec![])),
        }
    }

    fn eval_cons(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let car = self.eval(&tail[0])?;
        let cdr = self.eval(&tail[1])?;
        Ok(Value::Cons(Arc::new(Mutex::new(ConsCell { car, cdr }))))
    }

    fn eval_list_builtin(&mut self, tail: &[Expr]) -> EvalResult {
        let mut values = Vec::with_capacity(tail.len());
        for expr in tail {
            values.push(self.eval(expr)?);
        }
        Ok(list_from_vec(values))
    }

    fn eval_eq(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let left = self.eval(&tail[0])?;
        let right = self.eval(&tail[1])?;
        if eq_value(&left, &right) {
            Ok(Value::Symbol("t".to_string()))
        } else {
            Ok(Value::Nil)
        }
    }

    fn eval_equal(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let left = self.eval(&tail[0])?;
        let right = self.eval(&tail[1])?;
        if equal_value(&left, &right, 0) {
            Ok(Value::Symbol("t".to_string()))
        } else {
            Ok(Value::Nil)
        }
    }

    fn apply(&mut self, function: Value, args: Vec<Value>) -> EvalResult {
        match function {
            Value::Lambda(lambda) => {
                if lambda.params.len() != args.len() {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }

                let mut frame = HashMap::new();
                for (param, value) in lambda.params.iter().zip(args) {
                    frame.insert(param.clone(), value);
                }

                self.dynamic.push(frame);
                let result = self.eval_progn(&lambda.body);
                self.dynamic.pop();
                result
            }
            Value::Symbol(name) => self.apply_builtin(&name, args),
            _ => Err(signal("invalid-function", vec![function])),
        }
    }

    fn apply_named(&mut self, name: &str, args: Vec<Value>) -> EvalResult {
        let function = self.functions.get(name).cloned().ok_or_else(|| {
            signal("void-function", vec![Value::Symbol(name.to_string())])
        })?;
        self.apply(function, args)
    }

    fn apply_builtin(&mut self, name: &str, args: Vec<Value>) -> EvalResult {
        match name {
            "+" => {
                let mut sum = 0i64;
                for value in args {
                    sum = sum
                        .checked_add(expect_int(value)?)
                        .ok_or_else(|| signal("overflow-error", vec![]))?;
                }
                Ok(Value::Int(sum))
            }
            "/" => {
                if args.len() < 2 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                let mut iter = args.into_iter();
                let mut acc = expect_int(iter.next().expect("division arg exists"))?;
                for value in iter {
                    let divisor = expect_int(value)?;
                    if divisor == 0 {
                        return Err(signal("arith-error", vec![]));
                    }
                    acc /= divisor;
                }
                Ok(Value::Int(acc))
            }
            "car" => {
                if args.len() != 1 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                match &args[0] {
                    Value::Nil => Ok(Value::Nil),
                    Value::Cons(cell) => {
                        Ok(cell.lock().expect("cons cell mutex poisoned").car.clone())
                    }
                    _ => Err(signal("wrong-type-argument", vec![])),
                }
            }
            "cdr" => {
                if args.len() != 1 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                match &args[0] {
                    Value::Nil => Ok(Value::Nil),
                    Value::Cons(cell) => {
                        Ok(cell.lock().expect("cons cell mutex poisoned").cdr.clone())
                    }
                    _ => Err(signal("wrong-type-argument", vec![])),
                }
            }
            "cons" => {
                if args.len() != 2 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                Ok(Value::Cons(Arc::new(Mutex::new(ConsCell {
                    car: args[0].clone(),
                    cdr: args[1].clone(),
                }))))
            }
            "list" => Ok(list_from_vec(args)),
            "eq" => {
                if args.len() != 2 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                if eq_value(&args[0], &args[1]) {
                    Ok(Value::Symbol("t".to_string()))
                } else {
                    Ok(Value::Nil)
                }
            }
            "equal" => {
                if args.len() != 2 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                if equal_value(&args[0], &args[1], 0) {
                    Ok(Value::Symbol("t".to_string()))
                } else {
                    Ok(Value::Nil)
                }
            }
            "setcar" => {
                if args.len() != 2 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                match &args[0] {
                    Value::Cons(cell) => {
                        let mut pair = cell.lock().expect("cons cell mutex poisoned");
                        pair.car = args[1].clone();
                        Ok(args[1].clone())
                    }
                    _ => Err(signal("wrong-type-argument", vec![])),
                }
            }
            "setcdr" => {
                if args.len() != 2 {
                    return Err(signal("wrong-number-of-arguments", vec![]));
                }
                match &args[0] {
                    Value::Cons(cell) => {
                        let mut pair = cell.lock().expect("cons cell mutex poisoned");
                        pair.cdr = args[1].clone();
                        Ok(args[1].clone())
                    }
                    _ => Err(signal("wrong-type-argument", vec![])),
                }
            }
            _ => Err(signal("invalid-function", vec![Value::Symbol(name.to_string())])),
        }
    }

    fn eval_progn(&mut self, forms: &[Expr]) -> EvalResult {
        let mut last = Value::Nil;
        for form in forms {
            last = self.eval(form)?;
        }
        Ok(last)
    }

    fn assign(&mut self, name: &str, value: Value) {
        for frame in self.dynamic.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return;
            }
        }
        self.globals.insert(name.to_string(), value);
    }
}

fn expect_int(value: Value) -> Result<i64, Flow> {
    match value {
        Value::Int(v) => Ok(v),
        other => Err(signal("wrong-type-argument", vec![other])),
    }
}

fn signal(symbol: &str, data: Vec<Value>) -> Flow {
    Flow::Signal(SignalData {
        symbol: symbol.to_string(),
        data,
    })
}

fn signal_matches(pattern: &Expr, symbol: &str) -> bool {
    match pattern {
        Expr::Symbol(name) => name == symbol || name == "error" || name == "t",
        Expr::List(items) => items.iter().any(|item| signal_matches(item, symbol)),
        _ => false,
    }
}

fn make_signal_binding_value(sig: &SignalData) -> Value {
    let mut values = Vec::with_capacity(sig.data.len() + 1);
    values.push(Value::Symbol(sig.symbol.clone()));
    values.extend(sig.data.clone());
    list_from_vec(values)
}

fn map_flow(flow: Flow) -> EvalError {
    match flow {
        Flow::Signal(sig) => EvalError::Signal {
            symbol: sig.symbol,
            data: sig.data,
        },
        Flow::Throw { tag, value } => EvalError::UncaughtThrow { tag, value },
    }
}

pub fn parse_forms(input: &str) -> Result<Vec<Expr>, ParseError> {
    let mut parser = Parser::new(input);
    let mut forms = Vec::new();
    while parser.skip_ws_and_comments() {
        forms.push(parser.parse_expr()?);
    }
    Ok(forms)
}

pub fn eval_source(input: &str) -> Result<Vec<Result<Value, EvalError>>, ParseError> {
    let forms = parse_forms(input)?;
    let mut evaluator = Evaluator::new();
    Ok(evaluator.eval_forms(&forms))
}

pub fn print_expr(expr: &Expr) -> String {
    match expr {
        Expr::Int(v) => v.to_string(),
        Expr::Symbol(symbol) => symbol.clone(),
        Expr::List(items) => {
            if items.is_empty() {
                return "nil".to_string();
            }
            if items.len() == 2 {
                if let Expr::Symbol(symbol) = &items[0] {
                    if symbol == "quote" {
                        return format!("'{}", print_quoted_expr(&items[1]));
                    }
                }
            }
            let parts: Vec<String> = items.iter().map(print_expr).collect();
            format!("({})", parts.join(" "))
        }
    }
}

fn print_quoted_expr(expr: &Expr) -> String {
    match expr {
        Expr::List(items) if items.is_empty() => "nil".to_string(),
        _ => print_expr(expr),
    }
}

pub fn print_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Int(v) => v.to_string(),
        Value::Symbol(symbol) => symbol.clone(),
        Value::Cons(_) => {
            let mut out = String::from("(");
            print_cons(value, &mut out);
            out.push(')');
            out
        }
        Value::Lambda(lambda) => {
            let params = if lambda.params.is_empty() {
                "nil".to_string()
            } else {
                format!("({})", lambda.params.join(" "))
            };
            let body = lambda
                .body
                .iter()
                .map(print_expr)
                .collect::<Vec<_>>()
                .join(" ");
            format!("(lambda {} {})", params, body)
        }
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
                let pair = cell.lock().expect("cons cell mutex poisoned");
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

fn list_from_vec(values: Vec<Value>) -> Value {
    values.into_iter().rev().fold(Value::Nil, |acc, item| {
        Value::Cons(Arc::new(Mutex::new(ConsCell {
            car: item,
            cdr: acc,
        })))
    })
}

fn quote_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Int(v) => Value::Int(*v),
        Expr::Symbol(symbol) if symbol == "nil" => Value::Nil,
        Expr::Symbol(symbol) => Value::Symbol(symbol.clone()),
        Expr::List(items) => {
            let quoted = items.iter().map(quote_to_value).collect::<Vec<_>>();
            list_from_vec(quoted)
        }
    }
}

fn eq_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        (Value::Cons(a), Value::Cons(b)) => Arc::ptr_eq(a, b),
        (Value::Lambda(a), Value::Lambda(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

fn equal_value(left: &Value, right: &Value, depth: usize) -> bool {
    if depth > 4096 {
        return false;
    }

    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        (Value::Cons(a), Value::Cons(b)) => {
            if Arc::ptr_eq(a, b) {
                return true;
            }
            let left_cell = a.lock().expect("cons cell mutex poisoned");
            let right_cell = b.lock().expect("cons cell mutex poisoned");
            equal_value(&left_cell.car, &right_cell.car, depth + 1)
                && equal_value(&left_cell.cdr, &right_cell.cdr, depth + 1)
        }
        (Value::Lambda(a), Value::Lambda(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

fn is_truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil)
}

pub fn format_eval_result(result: &Result<Value, EvalError>) -> String {
    match result {
        Ok(value) => format!("OK {}", print_value(value)),
        Err(EvalError::Signal { symbol, data }) => {
            let payload = if data.is_empty() {
                "nil".to_string()
            } else {
                print_value(&list_from_vec(data.clone()))
            };
            format!("ERR ({} {})", symbol, payload)
        }
        Err(EvalError::UncaughtThrow { tag, value }) => {
            format!("ERR (no-catch ({} {}))", print_value(tag), print_value(value))
        }
    }
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn skip_ws_and_comments(&mut self) -> bool {
        loop {
            let Some(ch) = self.current() else {
                return false;
            };
            if ch.is_ascii_whitespace() {
                self.bump();
                continue;
            }
            if ch == ';' {
                while let Some(c) = self.current() {
                    self.bump();
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            return true;
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws_and_comments();
        let Some(ch) = self.current() else {
            return Err(self.error("unexpected end of input"));
        };

        match ch {
            '(' => self.parse_list(),
            ')' => Err(self.error("unexpected ')'")),
            '\'' => {
                self.bump();
                let quoted = self.parse_expr()?;
                Ok(Expr::List(vec![
                    Expr::Symbol("quote".to_string()),
                    quoted,
                ]))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_list(&mut self) -> Result<Expr, ParseError> {
        self.expect('(')?;
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.current() {
                Some(')') => {
                    self.bump();
                    break;
                }
                Some(_) => items.push(self.parse_expr()?),
                None => return Err(self.error("unterminated list")),
            }
        }
        Ok(Expr::List(items))
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        while let Some(ch) = self.current() {
            if ch.is_ascii_whitespace() || matches!(ch, '(' | ')' | '\'' | ';') {
                break;
            }
            self.bump();
        }

        if self.pos == start {
            return Err(self.error("expected atom"));
        }

        let token = &self.input[start..self.pos];
        if let Ok(number) = token.parse::<i64>() {
            return Ok(Expr::Int(number));
        }

        Ok(Expr::Symbol(token.to_string()))
    }

    fn expect(&mut self, expected: char) -> Result<(), ParseError> {
        match self.current() {
            Some(ch) if ch == expected => {
                self.bump();
                Ok(())
            }
            _ => Err(self.error(&format!("expected '{}'", expected))),
        }
    }

    fn current(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(ch) = self.current() {
            self.pos += ch.len_utf8();
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            position: self.pos,
            message: message.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_evaluates_core_forms() {
        let source = r#"
        (+ 1 2)
        (let ((x 1)) (setq x (+ x 2)) x)
        (let ((lst '(1 2))) (setcar lst 9) lst)
        (catch 'tag (throw 'tag 42))
        (condition-case e (/ 1 0) (arith-error 'div-zero))
        (let ((x 1))
          (let ((f (lambda () x)))
            (let ((x 2))
              (funcall f))))
        "#;

        let forms = parse_forms(source).expect("forms should parse");
        let mut evaluator = Evaluator::new();
        let results = evaluator.eval_forms(&forms);

        let rendered = results
            .iter()
            .map(format_eval_result)
            .collect::<Vec<_>>();

        assert_eq!(
            rendered,
            vec![
                "OK 3",
                "OK 3",
                "OK (9 2)",
                "OK 42",
                "OK div-zero",
                "OK 2"
            ]
        );
    }

    #[test]
    fn reports_void_variable() {
        let forms = parse_forms("(let ((x 1)) y)").expect("parse should succeed");
        let mut evaluator = Evaluator::new();
        let result = evaluator.eval_expr(&forms[0]);
        assert_eq!(format_eval_result(&result), "ERR (void-variable (y))");
    }

    #[test]
    fn supports_multiple_setq_pairs() {
        let forms = parse_forms("(let ((x 1) (y 2)) (setq x 10 y (+ x 3)) y)")
            .expect("parse should succeed");
        let mut evaluator = Evaluator::new();
        let result = evaluator.eval_expr(&forms[0]);
        assert_eq!(format_eval_result(&result), "OK 13");
    }

    #[test]
    fn supports_defun_and_direct_call() {
        let forms = parse_forms("(progn (defun add2 (a b) (+ a b)) (add2 5 7))")
            .expect("parse should succeed");
        let mut evaluator = Evaluator::new();
        let result = evaluator.eval_expr(&forms[0]);
        assert_eq!(format_eval_result(&result), "OK 12");
    }

    #[test]
    fn supports_if_and_list_primitives() {
        let forms = parse_forms("(if (eq (car (cons 1 nil)) 1) (list 9 8) nil)")
            .expect("parse should succeed");
        let mut evaluator = Evaluator::new();
        let result = evaluator.eval_expr(&forms[0]);
        assert_eq!(format_eval_result(&result), "OK (9 8)");
    }

    #[test]
    fn supports_let_star_and_equal() {
        let forms = parse_forms(
            "(let* ((x 1) (y (+ x 2))) (if (equal (list x y) '(1 3)) y 0))",
        )
        .expect("parse should succeed");
        let mut evaluator = Evaluator::new();
        let result = evaluator.eval_expr(&forms[0]);
        assert_eq!(format_eval_result(&result), "OK 3");
    }

    #[test]
    fn supports_setcdr_mutation() {
        let forms = parse_forms("(let ((x '(1 2 3))) (setcdr x '(9 10)) x)")
            .expect("parse should succeed");
        let mut evaluator = Evaluator::new();
        let result = evaluator.eval_expr(&forms[0]);
        assert_eq!(format_eval_result(&result), "OK (1 9 10)");
    }
}
