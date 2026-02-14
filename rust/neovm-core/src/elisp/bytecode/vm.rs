//! Bytecode virtual machine — stack-based interpreter.

use std::collections::HashMap;

use super::chunk::ByteCodeFunction;
use super::opcode::Op;
use crate::elisp::builtins;
use crate::elisp::error::*;
use crate::elisp::symbol::Obarray;
use crate::elisp::value::*;

/// Handler frame for catch/condition-case/unwind-protect.
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Handler {
    /// catch: tag value, jump target.
    Catch { tag: Value, target: u32 },
    /// condition-case: handler patterns, jump target.
    ConditionCase { target: u32 },
    /// unwind-protect: cleanup target.
    UnwindProtect { target: u32 },
}

/// The bytecode VM execution engine.
///
/// Operates on an Evaluator's obarray and dynamic binding stack.
pub struct Vm<'a> {
    obarray: &'a mut Obarray,
    dynamic: &'a mut Vec<HashMap<String, Value>>,
    lexenv: &'a mut Vec<HashMap<String, Value>>,
    #[allow(dead_code)]
    features: &'a mut Vec<String>,
    depth: usize,
    max_depth: usize,
}

impl<'a> Vm<'a> {
    pub fn new(
        obarray: &'a mut Obarray,
        dynamic: &'a mut Vec<HashMap<String, Value>>,
        lexenv: &'a mut Vec<HashMap<String, Value>>,
        features: &'a mut Vec<String>,
    ) -> Self {
        Self {
            obarray,
            dynamic,
            lexenv,
            features,
            depth: 0,
            max_depth: 200,
        }
    }

    /// Execute a bytecode function with given arguments.
    pub(crate) fn execute(&mut self, func: &ByteCodeFunction, args: Vec<Value>) -> EvalResult {
        self.depth += 1;
        if self.depth > self.max_depth {
            self.depth -= 1;
            return Err(signal(
                "excessive-lisp-nesting",
                vec![Value::Int(self.max_depth as i64)],
            ));
        }

        let result = self.run_frame(func, args);
        self.depth -= 1;
        result
    }

    fn run_frame(&mut self, func: &ByteCodeFunction, args: Vec<Value>) -> EvalResult {
        let mut stack: Vec<Value> = Vec::with_capacity(func.max_stack as usize);
        let mut pc: usize = 0;
        let mut handlers: Vec<Handler> = Vec::new();
        let mut bind_count: usize = 0;

        // Bind parameters
        let param_binds = self.bind_params(&func.params, args)?;
        if !param_binds.is_empty() {
            // If closure, push onto lexenv; otherwise dynamic
            if func.env.is_some() {
                // Restore captured env first
                if let Some(ref env) = func.env {
                    let saved_lexenv = std::mem::replace(self.lexenv, env.clone());
                    self.lexenv.push(param_binds);
                    let result =
                        self.run_loop(func, &mut stack, &mut pc, &mut handlers, &mut bind_count);
                    self.lexenv.pop();
                    *self.lexenv = saved_lexenv;
                    // Unbind dynamic bindings
                    for _ in 0..bind_count {
                        self.dynamic.pop();
                    }
                    return result;
                }
            }
            self.dynamic.push(param_binds);
            let result = self.run_loop(func, &mut stack, &mut pc, &mut handlers, &mut bind_count);
            self.dynamic.pop();
            // Unbind additional bindings from varbind ops
            for _ in 0..bind_count {
                self.dynamic.pop();
            }
            return result;
        }

        let result = self.run_loop(func, &mut stack, &mut pc, &mut handlers, &mut bind_count);
        for _ in 0..bind_count {
            self.dynamic.pop();
        }
        result
    }

    fn run_loop(
        &mut self,
        func: &ByteCodeFunction,
        stack: &mut Vec<Value>,
        pc: &mut usize,
        handlers: &mut Vec<Handler>,
        bind_count: &mut usize,
    ) -> EvalResult {
        let ops = &func.ops;
        let constants = &func.constants;

        while *pc < ops.len() {
            let op = &ops[*pc];
            *pc += 1;

            match op {
                // -- Constants and stack --
                Op::Constant(idx) => {
                    stack.push(constants[*idx as usize].clone());
                }
                Op::Nil => stack.push(Value::Nil),
                Op::True => stack.push(Value::True),
                Op::Pop => {
                    stack.pop();
                }
                Op::Dup => {
                    if let Some(top) = stack.last() {
                        stack.push(top.clone());
                    }
                }
                Op::StackRef(n) => {
                    let idx = stack.len().saturating_sub(1 + *n as usize);
                    stack.push(stack[idx].clone());
                }
                Op::StackSet(n) => {
                    if stack.is_empty() {
                        continue;
                    }
                    let n = *n as usize;
                    let val = stack.pop().unwrap_or(Value::Nil);
                    if n == 0 {
                        continue;
                    }
                    if n <= stack.len() {
                        let idx = stack.len() - n;
                        stack[idx] = val;
                    }
                }
                Op::DiscardN(raw) => {
                    let preserve_tos = (raw & 0x80) != 0;
                    let mut n = (raw & 0x7F) as usize;
                    if n == 0 {
                        continue;
                    }
                    n = n.min(stack.len());
                    if preserve_tos && n < stack.len() {
                        if let Some(top) = stack.last().cloned() {
                            let target = stack.len() - 1 - n;
                            stack[target] = top;
                        }
                    }
                    let new_len = stack.len().saturating_sub(n);
                    stack.truncate(new_len);
                }

                // -- Variable access --
                Op::VarRef(idx) => {
                    let name = sym_name(constants, *idx);
                    let val = self.lookup_var(&name)?;
                    stack.push(val);
                }
                Op::VarSet(idx) => {
                    let name = sym_name(constants, *idx);
                    let val = stack.pop().unwrap_or(Value::Nil);
                    self.assign_var(&name, val);
                }
                Op::VarBind(idx) => {
                    let name = sym_name(constants, *idx);
                    let val = stack.pop().unwrap_or(Value::Nil);
                    let mut frame = HashMap::new();
                    frame.insert(name, val);
                    self.dynamic.push(frame);
                    *bind_count += 1;
                }
                Op::Unbind(n) => {
                    for _ in 0..*n {
                        if *bind_count > 0 {
                            self.dynamic.pop();
                            *bind_count -= 1;
                        }
                    }
                }

                // -- Function calls --
                Op::Call(n) => {
                    let n = *n as usize;
                    let args_start = stack.len().saturating_sub(n);
                    let args: Vec<Value> = stack.drain(args_start..).collect();
                    let func_val = stack.pop().unwrap_or(Value::Nil);
                    match self.call_function(func_val, args) {
                        Ok(result) => stack.push(result),
                        Err(Flow::Throw { tag, value }) => {
                            if let Some(target) = resolve_throw_target(handlers, &tag) {
                                stack.push(value);
                                *pc = target as usize;
                                continue;
                            }
                            return Err(Flow::Throw { tag, value });
                        }
                        Err(flow) => return Err(flow),
                    }
                }
                Op::Apply(n) => {
                    let n = *n as usize;
                    if n == 0 {
                        let func_val = stack.pop().unwrap_or(Value::Nil);
                        match self.call_function(func_val, vec![]) {
                            Ok(result) => stack.push(result),
                            Err(Flow::Throw { tag, value }) => {
                                if let Some(target) = resolve_throw_target(handlers, &tag) {
                                    stack.push(value);
                                    *pc = target as usize;
                                    continue;
                                }
                                return Err(Flow::Throw { tag, value });
                            }
                            Err(flow) => return Err(flow),
                        }
                    } else {
                        let args_start = stack.len().saturating_sub(n);
                        let mut args: Vec<Value> = stack.drain(args_start..).collect();
                        let func_val = stack.pop().unwrap_or(Value::Nil);
                        // Spread last argument
                        if let Some(last) = args.pop() {
                            let spread = list_to_vec(&last).unwrap_or_default();
                            args.extend(spread);
                        }
                        match self.call_function(func_val, args) {
                            Ok(result) => stack.push(result),
                            Err(Flow::Throw { tag, value }) => {
                                if let Some(target) = resolve_throw_target(handlers, &tag) {
                                    stack.push(value);
                                    *pc = target as usize;
                                    continue;
                                }
                                return Err(Flow::Throw { tag, value });
                            }
                            Err(flow) => return Err(flow),
                        }
                    }
                }

                // -- Control flow --
                Op::Goto(addr) => {
                    *pc = *addr as usize;
                }
                Op::GotoIfNil(addr) => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    if val.is_nil() {
                        *pc = *addr as usize;
                    }
                }
                Op::GotoIfNotNil(addr) => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    if val.is_truthy() {
                        *pc = *addr as usize;
                    }
                }
                Op::GotoIfNilElsePop(addr) => {
                    if stack.last().map_or(true, |v| v.is_nil()) {
                        *pc = *addr as usize;
                    } else {
                        stack.pop();
                    }
                }
                Op::GotoIfNotNilElsePop(addr) => {
                    if stack.last().map_or(false, |v| v.is_truthy()) {
                        *pc = *addr as usize;
                    } else {
                        stack.pop();
                    }
                }
                Op::Return => {
                    return Ok(stack.pop().unwrap_or(Value::Nil));
                }

                // -- Arithmetic --
                Op::Add => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_add(&a, &b)?);
                }
                Op::Sub => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_sub(&a, &b)?);
                }
                Op::Mul => {
                    let b = stack.pop().unwrap_or(Value::Int(1));
                    let a = stack.pop().unwrap_or(Value::Int(1));
                    stack.push(arith_mul(&a, &b)?);
                }
                Op::Div => {
                    let b = stack.pop().unwrap_or(Value::Int(1));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_div(&a, &b)?);
                }
                Op::Rem => {
                    let b = stack.pop().unwrap_or(Value::Int(1));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_rem(&a, &b)?);
                }
                Op::Add1 => {
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_add1(&a)?);
                }
                Op::Sub1 => {
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_sub1(&a)?);
                }
                Op::Negate => {
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(arith_negate(&a)?);
                }

                // -- Comparison --
                Op::Eqlsign => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(Value::bool(num_eq(&a, &b)?));
                }
                Op::Gtr => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(Value::bool(num_cmp(&a, &b)? > 0));
                }
                Op::Lss => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(Value::bool(num_cmp(&a, &b)? < 0));
                }
                Op::Leq => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(Value::bool(num_cmp(&a, &b)? <= 0));
                }
                Op::Geq => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(Value::bool(num_cmp(&a, &b)? >= 0));
                }
                Op::Max => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(if num_cmp(&a, &b)? >= 0 { a } else { b });
                }
                Op::Min => {
                    let b = stack.pop().unwrap_or(Value::Int(0));
                    let a = stack.pop().unwrap_or(Value::Int(0));
                    stack.push(if num_cmp(&a, &b)? <= 0 { a } else { b });
                }

                // -- List operations --
                Op::Car => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(car(&val));
                }
                Op::Cdr => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(cdr(&val));
                }
                Op::CarSafe => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    match val {
                        Value::Cons(cell) => {
                            let pair = cell.lock().expect("poisoned");
                            stack.push(pair.car.clone());
                        }
                        _ => stack.push(Value::Nil),
                    }
                }
                Op::CdrSafe => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    match val {
                        Value::Cons(cell) => {
                            let pair = cell.lock().expect("poisoned");
                            stack.push(pair.cdr.clone());
                        }
                        _ => stack.push(Value::Nil),
                    }
                }
                Op::Cons => {
                    let cdr_val = stack.pop().unwrap_or(Value::Nil);
                    let car_val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::cons(car_val, cdr_val));
                }
                Op::List(n) => {
                    let n = *n as usize;
                    let start = stack.len().saturating_sub(n);
                    let items: Vec<Value> = stack.drain(start..).collect();
                    stack.push(Value::list(items));
                }
                Op::Length => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(length_value(&val)?);
                }
                Op::Nth => {
                    let list = stack.pop().unwrap_or(Value::Nil);
                    let n = stack.pop().unwrap_or(Value::Int(0));
                    let idx = n.as_int().unwrap_or(0);
                    stack.push(nth_value(idx, &list));
                }
                Op::Nthcdr => {
                    let list = stack.pop().unwrap_or(Value::Nil);
                    let n = stack.pop().unwrap_or(Value::Int(0));
                    let idx = n.as_int().unwrap_or(0);
                    stack.push(nthcdr_value(idx, &list));
                }
                Op::Elt => {
                    let idx = stack.pop().unwrap_or(Value::Nil);
                    let seq = stack.pop().unwrap_or(Value::Nil);
                    let result = self.dispatch_vm_builtin("elt", vec![seq, idx])?;
                    stack.push(result);
                }
                Op::Setcar => {
                    let newcar = stack.pop().unwrap_or(Value::Nil);
                    let cell = stack.pop().unwrap_or(Value::Nil);
                    if let Value::Cons(c) = &cell {
                        c.lock().expect("poisoned").car = newcar.clone();
                        stack.push(newcar);
                    } else {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("consp"), cell],
                        ));
                    }
                }
                Op::Setcdr => {
                    let newcdr = stack.pop().unwrap_or(Value::Nil);
                    let cell = stack.pop().unwrap_or(Value::Nil);
                    if let Value::Cons(c) = &cell {
                        c.lock().expect("poisoned").cdr = newcdr.clone();
                        stack.push(newcdr);
                    } else {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("consp"), cell],
                        ));
                    }
                }
                Op::Nconc => {
                    let b = stack.pop().unwrap_or(Value::Nil);
                    let a = stack.pop().unwrap_or(Value::Nil);
                    let result = self.dispatch_vm_builtin("nconc", vec![a, b])?;
                    stack.push(result);
                }
                Op::Nreverse => {
                    let list = stack.pop().unwrap_or(Value::Nil);
                    let result = self.dispatch_vm_builtin("nreverse", vec![list])?;
                    stack.push(result);
                }
                Op::Member => {
                    let list = stack.pop().unwrap_or(Value::Nil);
                    let elt = stack.pop().unwrap_or(Value::Nil);
                    let result = self.dispatch_vm_builtin("member", vec![elt, list])?;
                    stack.push(result);
                }
                Op::Memq => {
                    let list = stack.pop().unwrap_or(Value::Nil);
                    let elt = stack.pop().unwrap_or(Value::Nil);
                    stack.push(memq(&elt, &list));
                }
                Op::Assq => {
                    let alist = stack.pop().unwrap_or(Value::Nil);
                    let key = stack.pop().unwrap_or(Value::Nil);
                    stack.push(assq(&key, &alist));
                }

                // -- Type predicates --
                Op::Symbolp => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_symbol()));
                }
                Op::Consp => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_cons()));
                }
                Op::Stringp => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_string()));
                }
                Op::Listp => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_list()));
                }
                Op::Integerp => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_integer()));
                }
                Op::Numberp => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_number()));
                }
                Op::Null | Op::Not => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(val.is_nil()));
                }
                Op::Eq => {
                    let b = stack.pop().unwrap_or(Value::Nil);
                    let a = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(eq_value(&a, &b)));
                }
                Op::Equal => {
                    let b = stack.pop().unwrap_or(Value::Nil);
                    let a = stack.pop().unwrap_or(Value::Nil);
                    stack.push(Value::bool(equal_value(&a, &b, 0)));
                }

                // -- String operations --
                Op::Concat(n) => {
                    let n = *n as usize;
                    let start = stack.len().saturating_sub(n);
                    let parts: Vec<Value> = stack.drain(start..).collect();
                    let mut result = String::new();
                    for p in &parts {
                        match p {
                            Value::Str(s) => result.push_str(s),
                            Value::Symbol(s) => result.push_str(s),
                            Value::Nil => result.push_str("nil"),
                            Value::True => result.push_str("t"),
                            _ => result.push_str(&format!("{}", p)),
                        }
                    }
                    stack.push(Value::string(result));
                }
                Op::Substring => {
                    let to = stack.pop().unwrap_or(Value::Nil);
                    let from = stack.pop().unwrap_or(Value::Int(0));
                    let string = stack.pop().unwrap_or(Value::Nil);
                    let args = if to.is_nil() {
                        vec![string, from]
                    } else {
                        vec![string, from, to]
                    };
                    let result = self.dispatch_vm_builtin("substring", args)?;
                    stack.push(result);
                }
                Op::StringEqual => {
                    let b = stack.pop().unwrap_or(Value::Nil);
                    let a = stack.pop().unwrap_or(Value::Nil);
                    let eq = match (a.as_str(), b.as_str()) {
                        (Some(a), Some(b)) => a == b,
                        _ => false,
                    };
                    stack.push(Value::bool(eq));
                }
                Op::StringLessp => {
                    let b = stack.pop().unwrap_or(Value::Nil);
                    let a = stack.pop().unwrap_or(Value::Nil);
                    let lt = match (a.as_str(), b.as_str()) {
                        (Some(a), Some(b)) => a < b,
                        _ => false,
                    };
                    stack.push(Value::bool(lt));
                }

                // -- Vector operations --
                Op::Aref => {
                    let idx_val = stack.pop().unwrap_or(Value::Int(0));
                    let vec_val = stack.pop().unwrap_or(Value::Nil);
                    let idx = idx_val.as_int().unwrap_or(0) as usize;
                    match &vec_val {
                        Value::Vector(v) => {
                            let v = v.lock().expect("poisoned");
                            stack.push(v.get(idx).cloned().unwrap_or(Value::Nil));
                        }
                        _ => {
                            return Err(signal(
                                "wrong-type-argument",
                                vec![Value::symbol("arrayp"), vec_val],
                            ));
                        }
                    }
                }
                Op::Aset => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    let idx_val = stack.pop().unwrap_or(Value::Int(0));
                    let vec_val = stack.pop().unwrap_or(Value::Nil);
                    let idx = idx_val.as_int().unwrap_or(0) as usize;
                    match &vec_val {
                        Value::Vector(v) => {
                            let mut v = v.lock().expect("poisoned");
                            if idx < v.len() {
                                v[idx] = val.clone();
                            }
                            stack.push(val);
                        }
                        _ => {
                            return Err(signal(
                                "wrong-type-argument",
                                vec![Value::symbol("arrayp"), vec_val],
                            ));
                        }
                    }
                }

                // -- Symbol operations --
                Op::SymbolValue => {
                    let sym = stack.pop().unwrap_or(Value::Nil);
                    let name = sym.as_symbol_name().unwrap_or("nil");
                    match self.obarray.symbol_value(name) {
                        Some(val) => stack.push(val.clone()),
                        None => return Err(signal("void-variable", vec![sym])),
                    }
                }
                Op::SymbolFunction => {
                    let sym = stack.pop().unwrap_or(Value::Nil);
                    let name = sym.as_symbol_name().unwrap_or("nil");
                    match self.obarray.symbol_function(name) {
                        Some(val) => stack.push(val.clone()),
                        None => return Err(signal("void-function", vec![sym])),
                    }
                }
                Op::Set => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    let sym = stack.pop().unwrap_or(Value::Nil);
                    let name = sym.as_symbol_name().unwrap_or("nil").to_string();
                    self.obarray.set_symbol_value(&name, val.clone());
                    stack.push(val);
                }
                Op::Fset => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    let sym = stack.pop().unwrap_or(Value::Nil);
                    let name = sym.as_symbol_name().unwrap_or("nil").to_string();
                    self.obarray.set_symbol_function(&name, val.clone());
                    stack.push(val);
                }
                Op::Get => {
                    let prop = stack.pop().unwrap_or(Value::Nil);
                    let sym = stack.pop().unwrap_or(Value::Nil);
                    let sym_name = sym.as_symbol_name().unwrap_or("nil");
                    let prop_name = prop.as_symbol_name().unwrap_or("nil");
                    let val = self
                        .obarray
                        .get_property(sym_name, prop_name)
                        .cloned()
                        .unwrap_or(Value::Nil);
                    stack.push(val);
                }
                Op::Put => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    let prop = stack.pop().unwrap_or(Value::Nil);
                    let sym = stack.pop().unwrap_or(Value::Nil);
                    let sym_name = sym.as_symbol_name().unwrap_or("nil").to_string();
                    let prop_name = prop.as_symbol_name().unwrap_or("nil").to_string();
                    self.obarray
                        .put_property(&sym_name, &prop_name, val.clone());
                    stack.push(val);
                }

                // -- Error handling --
                Op::PushConditionCase(target) => {
                    handlers.push(Handler::ConditionCase { target: *target });
                }
                Op::PushConditionCaseRaw(target) => {
                    // GNU bytecode consumes the handler pattern operand from TOS.
                    stack.pop();
                    handlers.push(Handler::ConditionCase { target: *target });
                }
                Op::PushCatch(target) => {
                    let tag = stack.pop().unwrap_or(Value::Nil);
                    handlers.push(Handler::Catch {
                        tag,
                        target: *target,
                    });
                }
                Op::PopHandler => {
                    handlers.pop();
                }
                Op::UnwindProtect(target) => {
                    handlers.push(Handler::UnwindProtect { target: *target });
                }
                Op::Throw => {
                    let val = stack.pop().unwrap_or(Value::Nil);
                    let tag = stack.pop().unwrap_or(Value::Nil);
                    if let Some(target) = resolve_throw_target(handlers, &tag) {
                        stack.push(val);
                        *pc = target as usize;
                        continue;
                    }
                    return Err(Flow::Throw { tag, value: val });
                }

                // -- Closure --
                Op::MakeClosure(idx) => {
                    let val = constants[*idx as usize].clone();
                    if let Value::ByteCode(bc) = val {
                        let mut closure = (*bc).clone();
                        closure.env = Some(self.lexenv.clone());
                        stack.push(Value::ByteCode(std::sync::Arc::new(closure)));
                    } else {
                        stack.push(val);
                    }
                }

                // -- Builtin escape hatch --
                Op::CallBuiltin(name_idx, n) => {
                    let name = sym_name(constants, *name_idx);
                    let n = *n as usize;
                    let args_start = stack.len().saturating_sub(n);
                    let args: Vec<Value> = stack.drain(args_start..).collect();
                    let result = self.dispatch_vm_builtin(&name, args)?;
                    stack.push(result);
                }
            }
        }

        // Fell off the end — return TOS or nil
        Ok(stack.pop().unwrap_or(Value::Nil))
    }

    // -- Helper methods --

    fn lookup_var(&self, name: &str) -> EvalResult {
        if name == "nil" {
            return Ok(Value::Nil);
        }
        if name == "t" {
            return Ok(Value::True);
        }
        if name.starts_with(':') {
            return Ok(Value::Keyword(name.to_string()));
        }

        // Check lexenv
        for frame in self.lexenv.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Ok(val.clone());
            }
        }

        // Check dynamic
        for frame in self.dynamic.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Ok(val.clone());
            }
        }

        // Obarray
        if let Some(val) = self.obarray.symbol_value(name) {
            return Ok(val.clone());
        }

        Err(signal("void-variable", vec![Value::symbol(name)]))
    }

    fn assign_var(&mut self, name: &str, value: Value) {
        // Check lexenv
        for frame in self.lexenv.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return;
            }
        }
        // Check dynamic
        for frame in self.dynamic.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return;
            }
        }
        // Fall through to obarray
        self.obarray.set_symbol_value(name, value);
    }

    fn bind_params(
        &self,
        params: &LambdaParams,
        args: Vec<Value>,
    ) -> Result<HashMap<String, Value>, Flow> {
        let mut frame = HashMap::new();
        let mut arg_idx = 0;

        if args.len() < params.min_arity() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        if let Some(max) = params.max_arity() {
            if args.len() > max {
                return Err(signal("wrong-number-of-arguments", vec![]));
            }
        }

        for param in &params.required {
            frame.insert(param.clone(), args[arg_idx].clone());
            arg_idx += 1;
        }
        for param in &params.optional {
            if arg_idx < args.len() {
                frame.insert(param.clone(), args[arg_idx].clone());
                arg_idx += 1;
            } else {
                frame.insert(param.clone(), Value::Nil);
            }
        }
        if let Some(ref rest_name) = params.rest {
            let rest_args: Vec<Value> = args[arg_idx..].to_vec();
            frame.insert(rest_name.clone(), Value::list(rest_args));
        }
        Ok(frame)
    }

    fn call_function(&mut self, func_val: Value, args: Vec<Value>) -> EvalResult {
        match func_val {
            Value::ByteCode(bc) => self.execute(&bc, args),
            Value::Lambda(lambda) => {
                // Fall back to tree-walking for non-compiled lambdas
                // This creates a temporary evaluator context
                let frame = self.bind_params(&lambda.params, args)?;

                let saved_lexenv = if let Some(ref env) = lambda.env {
                    let old = std::mem::replace(self.lexenv, env.clone());
                    self.lexenv.push(frame);
                    Some(old)
                } else {
                    self.dynamic.push(frame);
                    None
                };

                // Execute lambda body forms
                let mut result = Value::Nil;
                for form in &lambda.body {
                    // We need to eval Expr — but we only have a VM.
                    // Compile the body on-the-fly and execute.
                    let mut compiler = super::compiler::Compiler::new(self.lexenv.len() > 0);
                    let compiled = compiler.compile_toplevel(form);
                    result = self.execute_inline(&compiled)?;
                }

                if let Some(old_lexenv) = saved_lexenv {
                    *self.lexenv = old_lexenv;
                } else {
                    self.dynamic.pop();
                }
                Ok(result)
            }
            Value::Subr(name) | Value::Symbol(name) => {
                // Try obarray function cell
                if let Some(func) = self.obarray.symbol_function(&name).cloned() {
                    return self.call_function(func, args);
                }
                // Try builtin
                self.dispatch_vm_builtin(&name, args)
            }
            _ => Err(signal("invalid-function", vec![func_val])),
        }
    }

    /// Execute a compiled function without param binding (for inline compilation).
    fn execute_inline(&mut self, func: &ByteCodeFunction) -> EvalResult {
        let mut stack: Vec<Value> = Vec::with_capacity(func.max_stack as usize);
        let mut pc: usize = 0;
        let mut handlers: Vec<Handler> = Vec::new();
        let mut bind_count: usize = 0;
        self.run_loop(func, &mut stack, &mut pc, &mut handlers, &mut bind_count)
    }

    /// Dispatch to builtin functions from the VM.
    fn dispatch_vm_builtin(&mut self, name: &str, args: Vec<Value>) -> EvalResult {
        // Handle special VM builtins
        match name {
            "%%defvar" => {
                // args: [init_value, symbol_name]
                if args.len() >= 2 {
                    let sym_name = args[1].as_symbol_name().unwrap_or("nil").to_string();
                    if !self.obarray.boundp(&sym_name) {
                        self.obarray.set_symbol_value(&sym_name, args[0].clone());
                    }
                    self.obarray.make_special(&sym_name);
                    return Ok(Value::symbol(sym_name));
                }
                return Ok(Value::Nil);
            }
            "%%defconst" => {
                if args.len() >= 2 {
                    let sym_name = args[1].as_symbol_name().unwrap_or("nil").to_string();
                    self.obarray.set_symbol_value(&sym_name, args[0].clone());
                    let sym = self.obarray.get_or_intern(&sym_name);
                    sym.constant = true;
                    sym.special = true;
                    return Ok(Value::symbol(sym_name));
                }
                return Ok(Value::Nil);
            }
            "%%unimplemented-elc-bytecode" => {
                return Err(signal(
                    "error",
                    vec![Value::string(
                        "Compiled .elc bytecode execution is not implemented yet",
                    )],
                ));
            }
            "throw" => {
                if args.len() != 2 {
                    return Err(signal(
                        "wrong-number-of-arguments",
                        vec![Value::Subr("throw".to_string()), Value::Int(args.len() as i64)],
                    ));
                }
                return Err(Flow::Throw {
                    tag: args[0].clone(),
                    value: args[1].clone(),
                });
            }
            _ => {}
        }

        // Create a temporary evaluator for builtin dispatch
        // This is a bridge: builtins that don't need the evaluator work fine,
        // those that do will need the evaluator reference.
        if let Some(result) = builtins::dispatch_builtin_pure(name, args.clone()) {
            return result;
        }

        Err(signal("void-function", vec![Value::symbol(name)]))
    }
}

// -- Arithmetic helpers --

fn resolve_throw_target(handlers: &mut Vec<Handler>, tag: &Value) -> Option<u32> {
    while let Some(handler) = handlers.pop() {
        if let Handler::Catch {
            tag: catch_tag,
            target,
        } = handler
        {
            if eq_value(&catch_tag, tag) {
                return Some(target);
            }
        }
    }
    None
}

fn arith_add(a: &Value, b: &Value) -> EvalResult {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(*b))),
        _ => {
            let a = a.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), a.clone()],
                )
            })?;
            let b = b.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), b.clone()],
                )
            })?;
            Ok(Value::Float(a + b))
        }
    }
}

fn arith_sub(a: &Value, b: &Value) -> EvalResult {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_sub(*b))),
        _ => {
            let a = a.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), a.clone()],
                )
            })?;
            let b = b.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), b.clone()],
                )
            })?;
            Ok(Value::Float(a - b))
        }
    }
}

fn arith_mul(a: &Value, b: &Value) -> EvalResult {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_mul(*b))),
        _ => {
            let a = a.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), a.clone()],
                )
            })?;
            let b = b.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), b.clone()],
                )
            })?;
            Ok(Value::Float(a * b))
        }
    }
}

fn arith_div(a: &Value, b: &Value) -> EvalResult {
    match (a, b) {
        (Value::Int(_), Value::Int(0)) => Err(signal(
            "arith-error",
            vec![Value::string("Division by zero")],
        )),
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
        _ => {
            let a = a.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), a.clone()],
                )
            })?;
            let b = b.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), b.clone()],
                )
            })?;
            if b == 0.0 {
                return Err(signal(
                    "arith-error",
                    vec![Value::string("Division by zero")],
                ));
            }
            Ok(Value::Float(a / b))
        }
    }
}

fn arith_rem(a: &Value, b: &Value) -> EvalResult {
    match (a, b) {
        (Value::Int(_), Value::Int(0)) => Err(signal(
            "arith-error",
            vec![Value::string("Division by zero")],
        )),
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), a.clone()],
        )),
    }
}

fn arith_add1(a: &Value) -> EvalResult {
    match a {
        Value::Int(n) => Ok(Value::Int(n.wrapping_add(1))),
        Value::Float(f) => Ok(Value::Float(f + 1.0)),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), a.clone()],
        )),
    }
}

fn arith_sub1(a: &Value) -> EvalResult {
    match a {
        Value::Int(n) => Ok(Value::Int(n.wrapping_sub(1))),
        Value::Float(f) => Ok(Value::Float(f - 1.0)),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), a.clone()],
        )),
    }
}

fn arith_negate(a: &Value) -> EvalResult {
    match a {
        Value::Int(n) => Ok(Value::Int(-n)),
        Value::Float(f) => Ok(Value::Float(-f)),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), a.clone()],
        )),
    }
}

fn num_eq(a: &Value, b: &Value) -> Result<bool, Flow> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        _ => {
            let a = a.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), a.clone()],
                )
            })?;
            let b = b.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), b.clone()],
                )
            })?;
            Ok(a == b)
        }
    }
}

fn num_cmp(a: &Value, b: &Value) -> Result<i32, Flow> {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b) as i32),
        _ => {
            let a = a.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), a.clone()],
                )
            })?;
            let b = b.as_number_f64().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), b.clone()],
                )
            })?;
            Ok(if a < b {
                -1
            } else if a > b {
                1
            } else {
                0
            })
        }
    }
}

// -- List helpers --

fn car(val: &Value) -> Value {
    match val {
        Value::Cons(c) => c.lock().expect("poisoned").car.clone(),
        Value::Nil => Value::Nil,
        _ => Value::Nil,
    }
}

fn cdr(val: &Value) -> Value {
    match val {
        Value::Cons(c) => c.lock().expect("poisoned").cdr.clone(),
        Value::Nil => Value::Nil,
        _ => Value::Nil,
    }
}

fn length_value(val: &Value) -> EvalResult {
    match val {
        Value::Nil => Ok(Value::Int(0)),
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Vector(v) => Ok(Value::Int(v.lock().expect("poisoned").len() as i64)),
        Value::Cons(_) => Ok(Value::Int(list_to_vec(val).map_or(0, |v| v.len()) as i64)),
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), val.clone()],
        )),
    }
}

fn nth_value(n: i64, list: &Value) -> Value {
    if n < 0 {
        return Value::Nil;
    }
    let mut cursor = list.clone();
    for _ in 0..n {
        cursor = cdr(&cursor);
    }
    car(&cursor)
}

fn nthcdr_value(n: i64, list: &Value) -> Value {
    if n <= 0 {
        return list.clone();
    }
    let mut cursor = list.clone();
    for _ in 0..n {
        cursor = cdr(&cursor);
    }
    cursor
}

fn memq(elt: &Value, list: &Value) -> Value {
    let mut cursor = list.clone();
    loop {
        match &cursor {
            Value::Cons(c) => {
                let pair = c.lock().expect("poisoned");
                if eq_value(elt, &pair.car) {
                    drop(pair);
                    return cursor;
                }
                let next = pair.cdr.clone();
                drop(pair);
                cursor = next;
            }
            _ => return Value::Nil,
        }
    }
}

fn assq(key: &Value, alist: &Value) -> Value {
    let mut cursor = alist.clone();
    loop {
        match &cursor {
            Value::Cons(c) => {
                let pair = c.lock().expect("poisoned");
                let found = if let Value::Cons(ref entry) = pair.car {
                    let entry = entry.lock().expect("poisoned");
                    if eq_value(key, &entry.car) {
                        Some(pair.car.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(found) = found {
                    return found;
                }
                let next = pair.cdr.clone();
                drop(pair);
                cursor = next;
            }
            _ => return Value::Nil,
        }
    }
}

fn sym_name(constants: &[Value], idx: u16) -> String {
    constants
        .get(idx as usize)
        .and_then(|v| v.as_symbol_name())
        .unwrap_or("nil")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::bytecode::compiler::Compiler;
    use crate::elisp::parse_forms;

    fn vm_eval(src: &str) -> Result<Value, EvalError> {
        let forms = parse_forms(src).expect("parse");
        let mut compiler = Compiler::new(false);
        let mut obarray = Obarray::new();
        // Set up standard variables
        obarray.set_symbol_value("most-positive-fixnum", Value::Int(i64::MAX));
        obarray.set_symbol_value("most-negative-fixnum", Value::Int(i64::MIN));

        let mut dynamic: Vec<HashMap<String, Value>> = Vec::new();
        let mut lexenv: Vec<HashMap<String, Value>> = Vec::new();
        let mut features: Vec<String> = Vec::new();

        let mut last = Value::Nil;
        for form in &forms {
            let func = compiler.compile_toplevel(form);
            let mut vm = Vm::new(&mut obarray, &mut dynamic, &mut lexenv, &mut features);
            last = vm.execute(&func, vec![]).map_err(map_flow)?;
        }
        Ok(last)
    }

    fn vm_eval_str(src: &str) -> String {
        match vm_eval(src) {
            Ok(val) => format!("OK {}", val),
            Err(e) => format!("ERR {:?}", e),
        }
    }

    #[test]
    fn vm_literal_int() {
        assert_eq!(vm_eval_str("42"), "OK 42");
    }

    #[test]
    fn vm_nil_t() {
        assert_eq!(vm_eval_str("nil"), "OK nil");
        assert_eq!(vm_eval_str("t"), "OK t");
    }

    #[test]
    fn vm_addition() {
        assert_eq!(vm_eval_str("(+ 1 2)"), "OK 3");
        assert_eq!(vm_eval_str("(+ 1 2 3)"), "OK 6");
    }

    #[test]
    fn vm_subtraction() {
        assert_eq!(vm_eval_str("(- 10 3)"), "OK 7");
        assert_eq!(vm_eval_str("(- 5)"), "OK -5");
    }

    #[test]
    fn vm_multiplication() {
        assert_eq!(vm_eval_str("(* 4 5)"), "OK 20");
    }

    #[test]
    fn vm_division() {
        assert_eq!(vm_eval_str("(/ 10 3)"), "OK 3");
    }

    #[test]
    fn vm_comparisons() {
        assert_eq!(vm_eval_str("(< 1 2)"), "OK t");
        assert_eq!(vm_eval_str("(> 1 2)"), "OK nil");
        assert_eq!(vm_eval_str("(= 3 3)"), "OK t");
        assert_eq!(vm_eval_str("(<= 3 3)"), "OK t");
        assert_eq!(vm_eval_str("(>= 5 3)"), "OK t");
    }

    #[test]
    fn vm_if() {
        assert_eq!(vm_eval_str("(if t 1 2)"), "OK 1");
        assert_eq!(vm_eval_str("(if nil 1 2)"), "OK 2");
        assert_eq!(vm_eval_str("(if nil 1)"), "OK nil");
    }

    #[test]
    fn vm_and_or() {
        assert_eq!(vm_eval_str("(and 1 2 3)"), "OK 3");
        assert_eq!(vm_eval_str("(and 1 nil 3)"), "OK nil");
        assert_eq!(vm_eval_str("(or nil nil 3)"), "OK 3");
        assert_eq!(vm_eval_str("(or nil nil)"), "OK nil");
    }

    #[test]
    fn vm_let() {
        assert_eq!(vm_eval_str("(let ((x 42)) x)"), "OK 42");
        assert_eq!(vm_eval_str("(let ((x 1) (y 2)) (+ x y))"), "OK 3");
    }

    #[test]
    fn vm_let_star() {
        assert_eq!(vm_eval_str("(let* ((x 1) (y (+ x 1))) y)"), "OK 2");
    }

    #[test]
    fn vm_setq() {
        assert_eq!(vm_eval_str("(progn (setq x 42) x)"), "OK 42");
    }

    #[test]
    fn vm_while_loop() {
        assert_eq!(
            vm_eval_str("(let ((x 0)) (while (< x 5) (setq x (1+ x))) x)"),
            "OK 5"
        );
    }

    #[test]
    fn vm_progn() {
        assert_eq!(vm_eval_str("(progn 1 2 3)"), "OK 3");
    }

    #[test]
    fn vm_prog1() {
        assert_eq!(vm_eval_str("(prog1 1 2 3)"), "OK 1");
    }

    #[test]
    fn vm_quote() {
        assert_eq!(vm_eval_str("'foo"), "OK foo");
        assert_eq!(vm_eval_str("'(1 2 3)"), "OK (1 2 3)");
    }

    #[test]
    fn vm_type_predicates() {
        assert_eq!(vm_eval_str("(null nil)"), "OK t");
        assert_eq!(vm_eval_str("(null 1)"), "OK nil");
        assert_eq!(vm_eval_str("(consp '(1 2))"), "OK t");
        assert_eq!(vm_eval_str("(integerp 42)"), "OK t");
        assert_eq!(vm_eval_str("(stringp \"hello\")"), "OK t");
    }

    #[test]
    fn vm_list_ops() {
        assert_eq!(vm_eval_str("(car '(1 2 3))"), "OK 1");
        assert_eq!(vm_eval_str("(cdr '(1 2 3))"), "OK (2 3)");
        assert_eq!(vm_eval_str("(cons 1 '(2 3))"), "OK (1 2 3)");
        assert_eq!(vm_eval_str("(length '(1 2 3))"), "OK 3");
        assert_eq!(vm_eval_str("(list 1 2 3)"), "OK (1 2 3)");
    }

    #[test]
    fn vm_eq_equal() {
        assert_eq!(vm_eval_str("(eq 'foo 'foo)"), "OK t");
        assert_eq!(vm_eval_str("(equal '(1 2) '(1 2))"), "OK t");
    }

    #[test]
    fn vm_concat() {
        assert_eq!(
            vm_eval_str(r#"(concat "hello" " " "world")"#),
            r#"OK "hello world""#
        );
    }

    #[test]
    fn vm_when_unless() {
        assert_eq!(vm_eval_str("(when t 1 2 3)"), "OK 3");
        assert_eq!(vm_eval_str("(when nil 1 2 3)"), "OK nil");
        assert_eq!(vm_eval_str("(unless nil 1 2 3)"), "OK 3");
        assert_eq!(vm_eval_str("(unless t 1 2 3)"), "OK nil");
    }

    #[test]
    fn vm_cond() {
        assert_eq!(vm_eval_str("(cond (nil 1) (t 2))"), "OK 2");
        assert_eq!(vm_eval_str("(cond (nil 1) (nil 2))"), "OK nil");
    }

    #[test]
    fn vm_nested_let() {
        assert_eq!(vm_eval_str("(let ((x 1)) (let ((y 2)) (+ x y)))"), "OK 3");
    }

    #[test]
    fn vm_vector_ops() {
        assert_eq!(vm_eval_str("(aref [10 20 30] 1)"), "OK 20");
        assert_eq!(vm_eval_str("(length [1 2 3])"), "OK 3");
    }

    #[test]
    fn vm_not_negation() {
        assert_eq!(vm_eval_str("(/= 1 2)"), "OK t");
        assert_eq!(vm_eval_str("(/= 1 1)"), "OK nil");
    }

    #[test]
    fn vm_float_arithmetic() {
        assert_eq!(vm_eval_str("(+ 1.0 2.0)"), "OK 3.0");
        assert_eq!(vm_eval_str("(+ 1 2.0)"), "OK 3.0");
    }

    #[test]
    fn vm_dotimes() {
        assert_eq!(
            vm_eval_str("(let ((sum 0)) (dotimes (i 5) (setq sum (+ sum i))) sum)"),
            "OK 10"
        );
    }

    #[test]
    fn vm_dolist() {
        assert_eq!(
            vm_eval_str(
                "(let ((result nil)) (dolist (x '(a b c)) (setq result (cons x result))) result)"
            ),
            "OK (c b a)"
        );
    }
}
