//! Bytecode compiler: transforms Expr AST into ByteCodeFunction.

use super::chunk::ByteCodeFunction;
use super::opcode::Op;
use crate::elisp::expr::Expr;
use crate::elisp::value::{LambdaParams, Value};

/// Compiler state.
pub struct Compiler {
    /// Whether lexical-binding is active.
    lexical: bool,
    /// Set of known special (dynamically-scoped) variable names.
    specials: Vec<String>,
}

impl Compiler {
    pub fn new(lexical: bool) -> Self {
        Self {
            lexical,
            specials: Vec::new(),
        }
    }

    /// Mark a variable as special (always dynamically bound).
    pub fn add_special(&mut self, name: &str) {
        if !self.specials.contains(&name.to_string()) {
            self.specials.push(name.to_string());
        }
    }

    #[allow(dead_code)]
    fn is_special(&self, name: &str) -> bool {
        self.specials.contains(&name.to_string())
    }

    /// Compile a top-level expression (not a function body).
    pub fn compile_toplevel(&mut self, expr: &Expr) -> ByteCodeFunction {
        let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        self.compile_expr(&mut func, expr, true);
        func.emit(Op::Return);
        self.compute_max_stack(&mut func);
        func
    }

    /// Compile a lambda expression into a ByteCodeFunction.
    pub fn compile_lambda(&mut self, params: &LambdaParams, body: &[Expr]) -> ByteCodeFunction {
        let mut func = ByteCodeFunction::new(params.clone());

        if body.is_empty() {
            func.emit(Op::Nil);
        } else {
            for (i, form) in body.iter().enumerate() {
                let is_last = i == body.len() - 1;
                self.compile_expr(&mut func, form, is_last);
                if !is_last {
                    func.emit(Op::Pop);
                }
            }
        }
        func.emit(Op::Return);
        self.compute_max_stack(&mut func);
        func
    }

    /// Compile a single expression.
    /// `for_value`: whether the result is needed on the stack.
    fn compile_expr(&mut self, func: &mut ByteCodeFunction, expr: &Expr, for_value: bool) {
        match expr {
            Expr::Int(n) => {
                if for_value {
                    let idx = func.add_constant(Value::Int(*n));
                    func.emit(Op::Constant(idx));
                }
            }
            Expr::Float(f) => {
                if for_value {
                    let idx = func.add_constant(Value::Float(*f));
                    func.emit(Op::Constant(idx));
                }
            }
            Expr::Str(s) => {
                if for_value {
                    let idx = func.add_constant(Value::string(s.clone()));
                    func.emit(Op::Constant(idx));
                }
            }
            Expr::Char(c) => {
                if for_value {
                    let idx = func.add_constant(Value::Char(*c));
                    func.emit(Op::Constant(idx));
                }
            }
            Expr::Keyword(s) => {
                if for_value {
                    let idx = func.add_constant(Value::Keyword(s.clone()));
                    func.emit(Op::Constant(idx));
                }
            }
            Expr::Bool(true) => {
                if for_value {
                    func.emit(Op::True);
                }
            }
            Expr::Bool(false) => {
                if for_value {
                    func.emit(Op::Nil);
                }
            }
            Expr::Symbol(name) => {
                if for_value {
                    self.compile_symbol_ref(func, name);
                }
            }
            Expr::Vector(items) => {
                if for_value {
                    // Compile each element, then wrap as vector constant
                    // For now, if all elements are constants, emit a single constant.
                    // Otherwise, fall back to runtime construction via builtin.
                    let all_const = items.iter().all(is_literal);
                    if all_const {
                        let vals: Vec<Value> = items.iter().map(literal_to_value).collect();
                        let idx = func.add_constant(Value::vector(vals));
                        func.emit(Op::Constant(idx));
                    } else {
                        // Compile each element, then call `vector` builtin
                        for item in items {
                            self.compile_expr(func, item, true);
                        }
                        let name_idx = func.add_symbol("vector");
                        func.emit(Op::CallBuiltin(name_idx, items.len() as u8));
                    }
                }
            }
            Expr::List(items) => {
                self.compile_list(func, items, for_value);
            }
            Expr::DottedList(items, _last) => {
                // Treat as regular list call (dotted lists in source are rare)
                self.compile_list(func, items, for_value);
            }
        }
    }

    fn compile_symbol_ref(&self, func: &mut ByteCodeFunction, name: &str) {
        match name {
            "nil" => func.emit(Op::Nil),
            "t" => func.emit(Op::True),
            _ if name.starts_with(':') => {
                let idx = func.add_constant(Value::Keyword(name.to_string()));
                func.emit(Op::Constant(idx));
            }
            _ => {
                let idx = func.add_symbol(name);
                func.emit(Op::VarRef(idx));
            }
        }
    }

    fn compile_list(&mut self, func: &mut ByteCodeFunction, items: &[Expr], for_value: bool) {
        if items.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let (head, tail) = items.split_first().unwrap();

        if let Expr::Symbol(name) = head {
            // Try special forms first
            if self.try_compile_special_form(func, name, tail, for_value) {
                return;
            }

            // Try dedicated opcodes for known builtins
            if for_value {
                if let Some(()) = self.try_compile_builtin_op(func, name, tail) {
                    return;
                }
            }

            // General function call
            // Push function reference, then args
            let name_idx = func.add_symbol(name);
            func.emit(Op::Constant(name_idx));
            for arg in tail {
                self.compile_expr(func, arg, true);
            }
            func.emit(Op::Call(tail.len() as u8));

            if !for_value {
                func.emit(Op::Pop);
            }
            return;
        }

        // Head is not a symbol — could be a lambda form
        if let Expr::List(lambda_form) = head {
            if let Some(Expr::Symbol(s)) = lambda_form.first() {
                if s == "lambda" {
                    self.compile_expr(func, head, true);
                    for arg in tail {
                        self.compile_expr(func, arg, true);
                    }
                    func.emit(Op::Call(tail.len() as u8));
                    if !for_value {
                        func.emit(Op::Pop);
                    }
                    return;
                }
            }
        }

        // Fallback: evaluate head as function
        self.compile_expr(func, head, true);
        for arg in tail {
            self.compile_expr(func, arg, true);
        }
        func.emit(Op::Call(tail.len() as u8));
        if !for_value {
            func.emit(Op::Pop);
        }
    }

    /// Returns true if the special form was handled.
    fn try_compile_special_form(
        &mut self,
        func: &mut ByteCodeFunction,
        name: &str,
        tail: &[Expr],
        for_value: bool,
    ) -> bool {
        match name {
            "quote" => {
                if for_value {
                    if let Some(expr) = tail.first() {
                        let val = literal_to_value(expr);
                        let idx = func.add_constant(val);
                        func.emit(Op::Constant(idx));
                    } else {
                        func.emit(Op::Nil);
                    }
                }
                true
            }
            "progn" => {
                self.compile_progn(func, tail, for_value);
                true
            }
            "prog1" => {
                if tail.is_empty() {
                    if for_value {
                        func.emit(Op::Nil);
                    }
                } else {
                    self.compile_expr(func, &tail[0], for_value);
                    for form in &tail[1..] {
                        self.compile_expr(func, form, false);
                    }
                }
                true
            }
            "if" => {
                self.compile_if(func, tail, for_value);
                true
            }
            "and" => {
                self.compile_and(func, tail, for_value);
                true
            }
            "or" => {
                self.compile_or(func, tail, for_value);
                true
            }
            "cond" => {
                self.compile_cond(func, tail, for_value);
                true
            }
            "while" => {
                self.compile_while(func, tail);
                if for_value {
                    func.emit(Op::Nil);
                }
                true
            }
            "let" => {
                self.compile_let(func, tail, for_value);
                true
            }
            "let*" => {
                self.compile_let_star(func, tail, for_value);
                true
            }
            "setq" => {
                self.compile_setq(func, tail, for_value);
                true
            }
            "defun" => {
                self.compile_defun(func, tail, for_value);
                true
            }
            "defvar" => {
                self.compile_defvar(func, tail, for_value);
                true
            }
            "defconst" => {
                self.compile_defconst(func, tail, for_value);
                true
            }
            "lambda" | "function" => {
                if for_value {
                    self.compile_lambda_or_function(func, name, tail);
                }
                true
            }
            "funcall" => {
                if tail.is_empty() {
                    if for_value {
                        func.emit(Op::Nil);
                    }
                } else {
                    self.compile_expr(func, &tail[0], true);
                    for arg in &tail[1..] {
                        self.compile_expr(func, arg, true);
                    }
                    func.emit(Op::Call(tail.len().saturating_sub(1) as u8));
                    if !for_value {
                        func.emit(Op::Pop);
                    }
                }
                true
            }
            "when" => {
                if tail.is_empty() {
                    if for_value {
                        func.emit(Op::Nil);
                    }
                } else {
                    // (when COND BODY...) => (if COND (progn BODY...))
                    self.compile_expr(func, &tail[0], true);
                    let jump_false = func.current_offset();
                    func.emit(Op::GotoIfNil(0)); // placeholder
                    self.compile_progn(func, &tail[1..], for_value);
                    let jump_end = func.current_offset();
                    func.emit(Op::Goto(0)); // placeholder
                    let else_target = func.current_offset();
                    func.patch_jump(jump_false, else_target);
                    if for_value {
                        func.emit(Op::Nil);
                    }
                    let end_target = func.current_offset();
                    func.patch_jump(jump_end, end_target);
                }
                true
            }
            "unless" => {
                if tail.is_empty() {
                    if for_value {
                        func.emit(Op::Nil);
                    }
                } else {
                    self.compile_expr(func, &tail[0], true);
                    let jump_true = func.current_offset();
                    func.emit(Op::GotoIfNotNil(0)); // placeholder
                    self.compile_progn(func, &tail[1..], for_value);
                    let jump_end = func.current_offset();
                    func.emit(Op::Goto(0)); // placeholder
                    let else_target = func.current_offset();
                    func.patch_jump(jump_true, else_target);
                    if for_value {
                        func.emit(Op::Nil);
                    }
                    let end_target = func.current_offset();
                    func.patch_jump(jump_end, end_target);
                }
                true
            }
            "catch" => {
                self.compile_catch(func, tail, for_value);
                true
            }
            "unwind-protect" => {
                self.compile_unwind_protect(func, tail, for_value);
                true
            }
            "condition-case" => {
                self.compile_condition_case(func, tail, for_value);
                true
            }
            "interactive" | "declare" => {
                // Ignored
                if for_value {
                    func.emit(Op::Nil);
                }
                true
            }
            "dotimes" => {
                self.compile_dotimes(func, tail, for_value);
                true
            }
            "dolist" => {
                self.compile_dolist(func, tail, for_value);
                true
            }
            "save-excursion" | "save-restriction" => {
                // Stub: just compile as progn
                self.compile_progn(func, tail, for_value);
                true
            }
            "with-current-buffer" => {
                // Stub: skip buffer arg, compile body
                if tail.is_empty() {
                    if for_value {
                        func.emit(Op::Nil);
                    }
                } else {
                    self.compile_expr(func, &tail[0], false); // eval buffer arg, discard
                    self.compile_progn(func, &tail[1..], for_value);
                }
                true
            }
            "ignore-errors" => {
                self.compile_ignore_errors(func, tail, for_value);
                true
            }
            _ => false,
        }
    }

    /// Try to compile a known builtin using a dedicated opcode.
    fn try_compile_builtin_op(
        &mut self,
        func: &mut ByteCodeFunction,
        name: &str,
        args: &[Expr],
    ) -> Option<()> {
        match (name, args.len()) {
            // Arithmetic (2 args)
            ("+", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Add);
                Some(())
            }
            ("-", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Sub);
                Some(())
            }
            ("*", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Mul);
                Some(())
            }
            ("/", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Div);
                Some(())
            }
            ("%", 2) | ("mod", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Rem);
                Some(())
            }
            ("1+", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Add1);
                Some(())
            }
            ("1-", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Sub1);
                Some(())
            }
            // Variadic + and *
            ("+", n) if n != 2 => {
                if n == 0 {
                    let idx = func.add_constant(Value::Int(0));
                    func.emit(Op::Constant(idx));
                } else {
                    self.compile_expr(func, &args[0], true);
                    for arg in &args[1..] {
                        self.compile_expr(func, arg, true);
                        func.emit(Op::Add);
                    }
                }
                Some(())
            }
            ("*", n) if n != 2 => {
                if n == 0 {
                    let idx = func.add_constant(Value::Int(1));
                    func.emit(Op::Constant(idx));
                } else {
                    self.compile_expr(func, &args[0], true);
                    for arg in &args[1..] {
                        self.compile_expr(func, arg, true);
                        func.emit(Op::Mul);
                    }
                }
                Some(())
            }
            ("-", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Negate);
                Some(())
            }
            // Comparisons
            ("=", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Eqlsign);
                Some(())
            }
            (">", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Gtr);
                Some(())
            }
            ("<", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Lss);
                Some(())
            }
            ("<=", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Leq);
                Some(())
            }
            (">=", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Geq);
                Some(())
            }
            ("/=", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Eqlsign);
                func.emit(Op::Not);
                Some(())
            }
            ("max", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Max);
                Some(())
            }
            ("min", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Min);
                Some(())
            }
            // List ops
            ("car", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Car);
                Some(())
            }
            ("cdr", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Cdr);
                Some(())
            }
            ("cons", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Cons);
                Some(())
            }
            ("list", _) => {
                for arg in args {
                    self.compile_expr(func, arg, true);
                }
                func.emit(Op::List(args.len() as u16));
                Some(())
            }
            ("length", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Length);
                Some(())
            }
            ("nth", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Nth);
                Some(())
            }
            ("nthcdr", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Nthcdr);
                Some(())
            }
            ("setcar", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Setcar);
                Some(())
            }
            ("setcdr", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Setcdr);
                Some(())
            }
            ("memq", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Memq);
                Some(())
            }
            ("assq", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Assq);
                Some(())
            }
            // Type predicates
            ("null", 1) | ("not", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Not);
                Some(())
            }
            ("symbolp", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Symbolp);
                Some(())
            }
            ("consp", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Consp);
                Some(())
            }
            ("stringp", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Stringp);
                Some(())
            }
            ("listp", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Listp);
                Some(())
            }
            ("integerp", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Integerp);
                Some(())
            }
            ("numberp", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::Numberp);
                Some(())
            }
            ("eq", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Eq);
                Some(())
            }
            ("equal", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Equal);
                Some(())
            }
            // String ops
            ("concat", _) => {
                for arg in args {
                    self.compile_expr(func, arg, true);
                }
                func.emit(Op::Concat(args.len() as u16));
                Some(())
            }
            ("substring", 2) | ("substring", 3) => {
                for arg in args {
                    self.compile_expr(func, arg, true);
                }
                // Use CallBuiltin for substring since it has variable args
                let name_idx = func.add_symbol("substring");
                func.emit(Op::CallBuiltin(name_idx, args.len() as u8));
                Some(())
            }
            ("string-equal", 2) | ("string=", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::StringEqual);
                Some(())
            }
            ("string-lessp", 2) | ("string<", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::StringLessp);
                Some(())
            }
            // Vector ops
            ("aref", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Aref);
                Some(())
            }
            ("aset", 3) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                self.compile_expr(func, &args[2], true);
                func.emit(Op::Aset);
                Some(())
            }
            // Symbol ops
            ("symbol-value", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::SymbolValue);
                Some(())
            }
            ("symbol-function", 1) => {
                self.compile_expr(func, &args[0], true);
                func.emit(Op::SymbolFunction);
                Some(())
            }
            ("set", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Set);
                Some(())
            }
            ("fset", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Fset);
                Some(())
            }
            ("get", 2) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                func.emit(Op::Get);
                Some(())
            }
            ("put", 3) => {
                self.compile_expr(func, &args[0], true);
                self.compile_expr(func, &args[1], true);
                self.compile_expr(func, &args[2], true);
                func.emit(Op::Put);
                Some(())
            }
            _ => None,
        }
    }

    // -- Special form compilation helpers ------------------------------------

    fn compile_progn(&mut self, func: &mut ByteCodeFunction, forms: &[Expr], for_value: bool) {
        if forms.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        for (i, form) in forms.iter().enumerate() {
            let is_last = i == forms.len() - 1;
            let need_value = if is_last { for_value } else { false };
            self.compile_expr(func, form, need_value);
            if !is_last && !need_value {
                // Value was not pushed, nothing to pop
            }
        }
    }

    fn compile_if(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.len() < 2 {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        // Compile condition
        self.compile_expr(func, &tail[0], true);
        let jump_false = func.current_offset();
        func.emit(Op::GotoIfNil(0)); // placeholder

        // Then branch
        self.compile_expr(func, &tail[1], for_value);
        let jump_end = func.current_offset();
        func.emit(Op::Goto(0)); // placeholder

        // Else branch
        let else_target = func.current_offset();
        func.patch_jump(jump_false, else_target);
        if tail.len() > 2 {
            self.compile_progn(func, &tail[2..], for_value);
        } else if for_value {
            func.emit(Op::Nil);
        }

        let end_target = func.current_offset();
        func.patch_jump(jump_end, end_target);
    }

    fn compile_and(&mut self, func: &mut ByteCodeFunction, forms: &[Expr], for_value: bool) {
        if forms.is_empty() {
            if for_value {
                func.emit(Op::True);
            }
            return;
        }

        let mut jump_patches = Vec::new();

        for (i, form) in forms.iter().enumerate() {
            let is_last = i == forms.len() - 1;
            self.compile_expr(func, form, true);

            if !is_last {
                if for_value {
                    let jump = func.current_offset();
                    func.emit(Op::GotoIfNilElsePop(0));
                    jump_patches.push(jump);
                } else {
                    let jump = func.current_offset();
                    func.emit(Op::GotoIfNil(0));
                    jump_patches.push(jump);
                }
            }
        }

        if !for_value {
            func.emit(Op::Pop);
        }

        let end = func.current_offset();
        for patch in jump_patches {
            func.patch_jump(patch, end);
        }
        if !for_value {
            // The nil-short-circuit jumps also need to land here
            // but they don't push a value in the !for_value case
        }
    }

    fn compile_or(&mut self, func: &mut ByteCodeFunction, forms: &[Expr], for_value: bool) {
        if forms.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let mut jump_patches = Vec::new();

        for (i, form) in forms.iter().enumerate() {
            let is_last = i == forms.len() - 1;
            self.compile_expr(func, form, true);

            if !is_last {
                if for_value {
                    let jump = func.current_offset();
                    func.emit(Op::GotoIfNotNilElsePop(0));
                    jump_patches.push(jump);
                } else {
                    let jump = func.current_offset();
                    func.emit(Op::GotoIfNotNil(0));
                    jump_patches.push(jump);
                }
            }
        }

        if !for_value {
            func.emit(Op::Pop);
        }

        let end = func.current_offset();
        for patch in jump_patches {
            func.patch_jump(patch, end);
        }
    }

    fn compile_cond(&mut self, func: &mut ByteCodeFunction, clauses: &[Expr], for_value: bool) {
        if clauses.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let mut end_patches = Vec::new();

        for (i, clause) in clauses.iter().enumerate() {
            let is_last = i == clauses.len() - 1;
            let Expr::List(items) = clause else {
                continue;
            };
            if items.is_empty() {
                continue;
            }

            // Compile test
            self.compile_expr(func, &items[0], true);

            if items.len() == 1 {
                // (cond (TEST)) - return test value if true
                if is_last {
                    if !for_value {
                        func.emit(Op::Pop);
                    }
                } else {
                    if for_value {
                        let jump = func.current_offset();
                        func.emit(Op::GotoIfNotNilElsePop(0));
                        end_patches.push(jump);
                    } else {
                        let jump = func.current_offset();
                        func.emit(Op::GotoIfNotNil(0));
                        end_patches.push(jump);
                    }
                }
            } else {
                // (cond (TEST BODY...))
                if is_last {
                    // Last clause: run body if test passes, nil if not
                    let jump_skip = func.current_offset();
                    func.emit(Op::GotoIfNil(0));
                    self.compile_progn(func, &items[1..], for_value);
                    let jump_end = func.current_offset();
                    func.emit(Op::Goto(0)); // jump past trailing nil
                    end_patches.push(jump_end);
                    let skip_target = func.current_offset();
                    func.patch_jump(jump_skip, skip_target);
                } else {
                    let jump_skip = func.current_offset();
                    func.emit(Op::GotoIfNil(0));
                    self.compile_progn(func, &items[1..], for_value);
                    let jump_end = func.current_offset();
                    func.emit(Op::Goto(0));
                    end_patches.push(jump_end);
                    let skip_target = func.current_offset();
                    func.patch_jump(jump_skip, skip_target);
                }
            }
        }

        // All remaining clauses fell through — push nil if needed
        if for_value {
            func.emit(Op::Nil);
        }

        let end = func.current_offset();
        for patch in end_patches {
            func.patch_jump(patch, end);
        }
    }

    fn compile_while(&mut self, func: &mut ByteCodeFunction, tail: &[Expr]) {
        if tail.is_empty() {
            return;
        }
        let loop_start = func.current_offset();
        self.compile_expr(func, &tail[0], true);
        let exit_jump = func.current_offset();
        func.emit(Op::GotoIfNil(0));

        for form in &tail[1..] {
            self.compile_expr(func, form, false);
        }
        func.emit(Op::Goto(loop_start));

        let exit_target = func.current_offset();
        func.patch_jump(exit_jump, exit_target);
    }

    fn compile_let(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let mut bind_count = 0u16;

        match &tail[0] {
            Expr::List(entries) => {
                // Evaluate all init values first (parallel let)
                let mut names = Vec::new();
                for binding in entries {
                    match binding {
                        Expr::Symbol(name) => {
                            func.emit(Op::Nil);
                            names.push(name.clone());
                        }
                        Expr::List(pair) if !pair.is_empty() => {
                            let Expr::Symbol(name) = &pair[0] else {
                                continue;
                            };
                            if pair.len() > 1 {
                                self.compile_expr(func, &pair[1], true);
                            } else {
                                func.emit(Op::Nil);
                            }
                            names.push(name.clone());
                        }
                        _ => {}
                    }
                }
                // Now bind them all
                for name in names.iter().rev() {
                    let idx = func.add_symbol(name);
                    func.emit(Op::VarBind(idx));
                    bind_count += 1;
                }
            }
            Expr::Symbol(s) if s == "nil" => {} // (let nil ...)
            _ => {}
        }

        self.compile_progn(func, &tail[1..], for_value);

        if bind_count > 0 {
            func.emit(Op::Unbind(bind_count));
        }
    }

    fn compile_let_star(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let mut bind_count = 0u16;

        match &tail[0] {
            Expr::List(entries) => {
                for binding in entries {
                    match binding {
                        Expr::Symbol(name) => {
                            func.emit(Op::Nil);
                            let idx = func.add_symbol(name);
                            func.emit(Op::VarBind(idx));
                            bind_count += 1;
                        }
                        Expr::List(pair) if !pair.is_empty() => {
                            let Expr::Symbol(name) = &pair[0] else {
                                continue;
                            };
                            if pair.len() > 1 {
                                self.compile_expr(func, &pair[1], true);
                            } else {
                                func.emit(Op::Nil);
                            }
                            let idx = func.add_symbol(name);
                            func.emit(Op::VarBind(idx));
                            bind_count += 1;
                        }
                        _ => {}
                    }
                }
            }
            Expr::Symbol(s) if s == "nil" => {}
            _ => {}
        }

        self.compile_progn(func, &tail[1..], for_value);

        if bind_count > 0 {
            func.emit(Op::Unbind(bind_count));
        }
    }

    fn compile_setq(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let mut i = 0;
        while i + 1 < tail.len() {
            let Expr::Symbol(name) = &tail[i] else {
                i += 2;
                continue;
            };
            self.compile_expr(func, &tail[i + 1], true);
            let is_last_pair = i + 2 >= tail.len();
            if for_value && is_last_pair {
                func.emit(Op::Dup);
            }
            let idx = func.add_symbol(name);
            func.emit(Op::VarSet(idx));
            i += 2;
        }
    }

    fn compile_defun(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.len() < 3 {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::Symbol(name) = &tail[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };

        // Compile the lambda body (skip docstring)
        let body_start = if tail.len() > 3 {
            if let Expr::Str(_) = &tail[2] {
                3
            } else {
                2
            }
        } else {
            2
        };

        // Parse params
        let params = parse_params(&tail[1]);

        // Compile nested lambda as bytecode
        let inner = self.compile_lambda(&params, &tail[body_start..]);

        // Store the compiled function as a constant
        let bytecode_val = Value::ByteCode(std::sync::Arc::new(inner));
        let func_idx = func.add_constant(bytecode_val);
        let name_idx = func.add_symbol(name);

        // (fset 'name <compiled-function>)
        func.emit(Op::Constant(name_idx));
        func.emit(Op::Constant(func_idx));
        func.emit(Op::Fset);

        if for_value {
            func.emit(Op::Constant(name_idx));
        } else {
            func.emit(Op::Pop); // fset returns the value, discard it
        }
    }

    fn compile_defvar(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::Symbol(name) = &tail[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };

        self.add_special(name);

        // defvar only sets if not already bound — use CallBuiltin
        // We compile this as a runtime call to preserve the "only set if unbound" semantics
        let name_idx = func.add_symbol(name);
        if tail.len() > 1 {
            self.compile_expr(func, &tail[1], true);
        } else {
            func.emit(Op::Nil);
        }
        let defvar_name = func.add_symbol("%%defvar");
        func.emit(Op::Constant(name_idx)); // symbol name
                                           // Stack: [init-value, symbol-name]
                                           // Swap order for defvar builtin: needs (name value)
        func.emit(Op::CallBuiltin(defvar_name, 2));
        if !for_value {
            func.emit(Op::Pop);
        }
    }

    fn compile_defconst(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.len() < 2 {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::Symbol(name) = &tail[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };

        self.add_special(name);

        let name_idx = func.add_symbol(name);
        self.compile_expr(func, &tail[1], true);
        let defconst_name = func.add_symbol("%%defconst");
        func.emit(Op::Constant(name_idx));
        func.emit(Op::CallBuiltin(defconst_name, 2));
        if !for_value {
            func.emit(Op::Pop);
        }
    }

    fn compile_lambda_or_function(
        &mut self,
        func: &mut ByteCodeFunction,
        name: &str,
        tail: &[Expr],
    ) {
        if name == "function" {
            // #'symbol or #'(lambda ...)
            if let Some(Expr::Symbol(sym)) = tail.first() {
                // #'symbol — push function reference
                let idx = func.add_symbol(sym);
                func.emit(Op::Constant(idx));
                return;
            }
            if let Some(Expr::List(items)) = tail.first() {
                if let Some(Expr::Symbol(s)) = items.first() {
                    if s == "lambda" {
                        // #'(lambda ...)
                        self.compile_raw_lambda(func, &items[1..]);
                        return;
                    }
                }
            }
            func.emit(Op::Nil);
        } else {
            // bare `lambda`
            self.compile_raw_lambda(func, tail);
        }
    }

    fn compile_raw_lambda(&mut self, func: &mut ByteCodeFunction, tail: &[Expr]) {
        if tail.is_empty() {
            func.emit(Op::Nil);
            return;
        }

        let params = parse_params(&tail[0]);
        let body_start = if tail.len() > 2 {
            if let Expr::Str(_) = &tail[1] {
                2
            } else {
                1
            }
        } else {
            1
        };

        let inner = self.compile_lambda(&params, &tail[body_start..]);
        let bytecode_val = Value::ByteCode(std::sync::Arc::new(inner));

        if self.lexical {
            let idx = func.add_constant(bytecode_val);
            func.emit(Op::MakeClosure(idx));
        } else {
            let idx = func.add_constant(bytecode_val);
            func.emit(Op::Constant(idx));
        }
    }

    fn compile_catch(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        // Compile tag
        self.compile_expr(func, &tail[0], true);

        // Push handler
        let handler_jump = func.current_offset();
        func.emit(Op::PushConditionCase(0)); // placeholder

        // Compile body
        self.compile_progn(func, &tail[1..], true);
        func.emit(Op::PopHandler);
        let end_jump = func.current_offset();
        func.emit(Op::Goto(0)); // placeholder

        // Handler target: error value is on stack
        let handler_target = func.current_offset();
        func.patch_jump(handler_jump, handler_target);

        // The catch handler will have the caught value on stack
        let end_target = func.current_offset();
        func.patch_jump(end_jump, end_target);

        if !for_value {
            func.emit(Op::Pop);
        }
    }

    fn compile_unwind_protect(
        &mut self,
        func: &mut ByteCodeFunction,
        tail: &[Expr],
        for_value: bool,
    ) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        let cleanup_jump = func.current_offset();
        func.emit(Op::UnwindProtect(0)); // placeholder

        // Protected form
        self.compile_expr(func, &tail[0], for_value);

        // Pop the unwind-protect handler
        func.emit(Op::PopHandler);

        // Run cleanup forms (result discarded)
        for form in &tail[1..] {
            self.compile_expr(func, form, false);
        }

        let skip_cleanup = func.current_offset();
        func.emit(Op::Goto(0)); // skip cleanup re-execution on normal path

        // Cleanup target (entered on non-local exit)
        let cleanup_target = func.current_offset();
        func.patch_jump(cleanup_jump, cleanup_target);
        for form in &tail[1..] {
            self.compile_expr(func, form, false);
        }
        // Re-throw or continue

        let end = func.current_offset();
        func.patch_jump(skip_cleanup, end);
    }

    fn compile_condition_case(
        &mut self,
        func: &mut ByteCodeFunction,
        tail: &[Expr],
        for_value: bool,
    ) {
        if tail.len() < 3 {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }

        // Push handler
        let handler_jump = func.current_offset();
        func.emit(Op::PushConditionCase(0)); // placeholder

        // Body
        self.compile_expr(func, &tail[1], true);
        func.emit(Op::PopHandler);
        let end_jump = func.current_offset();
        func.emit(Op::Goto(0)); // jump past handlers

        // Handlers
        let handler_target = func.current_offset();
        func.patch_jump(handler_jump, handler_target);

        // Error value is on stack. Bind to variable if needed.
        let _var = match &tail[0] {
            Expr::Symbol(name) if name != "nil" => Some(name.clone()),
            _ => None,
        };

        // For simplicity, bind error value and compile handler bodies
        // In practice, condition-case selects handler by error type, but
        // for the VM we do this at runtime via the VM's handler mechanism.
        // Just compile the first handler's body.
        if let Some(Expr::List(handler_items)) = tail.get(2) {
            if handler_items.len() > 1 {
                if let Some(ref var_name) = _var {
                    let var_idx = func.add_symbol(var_name);
                    func.emit(Op::VarBind(var_idx));
                    self.compile_progn(func, &handler_items[1..], for_value);
                    func.emit(Op::Unbind(1));
                } else {
                    func.emit(Op::Pop); // discard error value
                    self.compile_progn(func, &handler_items[1..], for_value);
                }
            } else {
                func.emit(Op::Pop);
                if for_value {
                    func.emit(Op::Nil);
                }
            }
        } else {
            func.emit(Op::Pop);
            if for_value {
                func.emit(Op::Nil);
            }
        }

        let end_target = func.current_offset();
        func.patch_jump(end_jump, end_target);
    }

    fn compile_ignore_errors(
        &mut self,
        func: &mut ByteCodeFunction,
        tail: &[Expr],
        for_value: bool,
    ) {
        // Push handler
        let handler_jump = func.current_offset();
        func.emit(Op::PushConditionCase(0));

        // Body
        self.compile_progn(func, tail, for_value);
        func.emit(Op::PopHandler);
        let end_jump = func.current_offset();
        func.emit(Op::Goto(0));

        // Error handler: push nil
        let handler_target = func.current_offset();
        func.patch_jump(handler_jump, handler_target);
        func.emit(Op::Pop); // discard error
        if for_value {
            func.emit(Op::Nil);
        }

        let end = func.current_offset();
        func.patch_jump(end_jump, end);
    }

    fn compile_dotimes(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::List(spec) = &tail[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };
        if spec.len() < 2 {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::Symbol(var) = &spec[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };

        // Lower to: (let ((VAR 0)) (while (< VAR COUNT) BODY (setq VAR (1+ VAR))))
        let while_cond = Expr::List(vec![
            Expr::Symbol("<".into()),
            Expr::Symbol(var.clone()),
            spec[1].clone(),
        ]);
        let incr = Expr::List(vec![
            Expr::Symbol("setq".into()),
            Expr::Symbol(var.clone()),
            Expr::List(vec![Expr::Symbol("1+".into()), Expr::Symbol(var.clone())]),
        ]);

        // Bind var = 0
        let init_val = Expr::Int(0);
        let binding = Expr::List(vec![Expr::Symbol(var.clone()), init_val]);

        let mut body_forms: Vec<Expr> = tail[1..].to_vec();
        body_forms.push(incr);

        let mut while_forms = vec![while_cond];
        while_forms.extend(body_forms);

        let let_form = Expr::List({
            let mut items = vec![Expr::Symbol("let".into())];
            items.push(Expr::List(vec![binding]));
            items.push(Expr::List({
                let mut w = vec![Expr::Symbol("while".into())];
                w.extend(while_forms);
                w
            }));
            // Result
            if spec.len() > 2 {
                items.push(spec[2].clone());
            }
            items
        });

        self.compile_expr(func, &let_form, for_value);
    }

    fn compile_dolist(&mut self, func: &mut ByteCodeFunction, tail: &[Expr], for_value: bool) {
        if tail.is_empty() {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::List(spec) = &tail[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };
        if spec.len() < 2 {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        }
        let Expr::Symbol(var) = &spec[0] else {
            if for_value {
                func.emit(Op::Nil);
            }
            return;
        };

        // Synthesize as:
        // (let ((__dolist_tail__ LIST) (VAR nil))
        //   (while __dolist_tail__
        //     (setq VAR (car __dolist_tail__))
        //     BODY...
        //     (setq __dolist_tail__ (cdr __dolist_tail__)))
        //   RESULT)
        let tail_var = "__dolist_tail__".to_string();

        let binding_tail = Expr::List(vec![Expr::Symbol(tail_var.clone()), spec[1].clone()]);
        let binding_var = Expr::List(vec![Expr::Symbol(var.clone()), Expr::Symbol("nil".into())]);

        let setq_var = Expr::List(vec![
            Expr::Symbol("setq".into()),
            Expr::Symbol(var.clone()),
            Expr::List(vec![
                Expr::Symbol("car".into()),
                Expr::Symbol(tail_var.clone()),
            ]),
        ]);

        let advance_tail = Expr::List(vec![
            Expr::Symbol("setq".into()),
            Expr::Symbol(tail_var.clone()),
            Expr::List(vec![
                Expr::Symbol("cdr".into()),
                Expr::Symbol(tail_var.clone()),
            ]),
        ]);

        let mut while_body = vec![setq_var];
        while_body.extend_from_slice(&tail[1..]);
        while_body.push(advance_tail);

        let while_form = Expr::List({
            let mut w = vec![Expr::Symbol("while".into())];
            w.push(Expr::Symbol(tail_var.clone()));
            w.extend(while_body);
            w
        });

        let let_form = Expr::List({
            let mut items = vec![Expr::Symbol("let*".into())];
            items.push(Expr::List(vec![binding_tail, binding_var]));
            items.push(while_form);
            if spec.len() > 2 {
                items.push(spec[2].clone());
            }
            items
        });

        self.compile_expr(func, &let_form, for_value);
    }

    /// Compute max stack depth by walking the instruction stream.
    fn compute_max_stack(&self, func: &mut ByteCodeFunction) {
        let mut depth: i32 = 0;
        let mut max: i32 = 0;

        for op in &func.ops {
            let delta = stack_delta(op);
            depth += delta;
            if depth > max {
                max = depth;
            }
            // Don't let depth go negative (indicates a bug, but be safe)
            if depth < 0 {
                depth = 0;
            }
        }

        func.max_stack = max.max(1) as u16;
    }
}

/// Parse an Expr into LambdaParams.
fn parse_params(expr: &Expr) -> LambdaParams {
    match expr {
        Expr::Symbol(s) if s == "nil" => LambdaParams::simple(vec![]),
        Expr::List(items) => {
            let mut required = Vec::new();
            let mut optional = Vec::new();
            let mut rest = None;
            let mut mode = 0;

            for item in items {
                let Expr::Symbol(name) = item else { continue };
                match name.as_str() {
                    "&optional" => {
                        mode = 1;
                        continue;
                    }
                    "&rest" => {
                        mode = 2;
                        continue;
                    }
                    _ => {}
                }
                match mode {
                    0 => required.push(name.clone()),
                    1 => optional.push(name.clone()),
                    2 => {
                        rest = Some(name.clone());
                        break;
                    }
                    _ => {}
                }
            }
            LambdaParams {
                required,
                optional,
                rest,
            }
        }
        _ => LambdaParams::simple(vec![]),
    }
}

/// Check if an expression is a literal (constant) that doesn't need evaluation.
fn is_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Int(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Char(_)
            | Expr::Keyword(_)
            | Expr::Bool(_)
    ) || matches!(expr, Expr::Symbol(s) if s == "nil" || s == "t")
}

/// Convert a literal expression to a Value.
fn literal_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Int(n) => Value::Int(*n),
        Expr::Float(f) => Value::Float(*f),
        Expr::Str(s) => Value::string(s.clone()),
        Expr::Char(c) => Value::Char(*c),
        Expr::Keyword(s) => Value::Keyword(s.clone()),
        Expr::Bool(true) => Value::True,
        Expr::Bool(false) => Value::Nil,
        Expr::Symbol(s) if s == "nil" => Value::Nil,
        Expr::Symbol(s) if s == "t" => Value::True,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) if items.is_empty() => Value::Nil,
        Expr::List(items) => {
            // For quoted list, recursively convert
            if items.len() == 2 {
                if let Expr::Symbol(s) = &items[0] {
                    if s == "quote" {
                        return literal_to_value(&items[1]);
                    }
                }
            }
            let vals: Vec<Value> = items.iter().map(literal_to_value).collect();
            Value::list(vals)
        }
        Expr::Vector(items) => {
            let vals: Vec<Value> = items.iter().map(literal_to_value).collect();
            Value::vector(vals)
        }
        Expr::DottedList(items, last) => {
            let head_vals: Vec<Value> = items.iter().map(literal_to_value).collect();
            let tail_val = literal_to_value(last);
            head_vals
                .into_iter()
                .rev()
                .fold(tail_val, |acc, item| Value::cons(item, acc))
        }
    }
}

/// Stack depth change for an operation.
fn stack_delta(op: &Op) -> i32 {
    match op {
        Op::Constant(_) | Op::Nil | Op::True | Op::Dup | Op::StackRef(_) => 1,
        Op::Pop => -1,
        Op::VarRef(_) => 1,
        Op::VarSet(_) => -1,
        Op::VarBind(_) => -1,
        Op::Unbind(_) => 0,
        Op::Call(n) => -(*n as i32), // pops func + n args, pushes result
        Op::Apply(n) => -(*n as i32),
        Op::Goto(_) => 0,
        Op::GotoIfNil(_) | Op::GotoIfNotNil(_) => -1,
        Op::GotoIfNilElsePop(_) | Op::GotoIfNotNilElsePop(_) => 0, // conditional pop
        Op::Return => -1,
        // Binary ops: pop 2, push 1
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem => -1,
        Op::Add1 | Op::Sub1 | Op::Negate => 0, // pop 1, push 1
        Op::Eqlsign | Op::Gtr | Op::Lss | Op::Leq | Op::Geq | Op::Max | Op::Min => -1,
        Op::Car | Op::Cdr => 0,
        Op::Cons => -1,
        Op::List(n) => -(*n as i32) + 1,
        Op::Length => 0,
        Op::Nth | Op::Nthcdr | Op::Member | Op::Memq | Op::Assq | Op::Nconc => -1,
        Op::Nreverse => 0,
        Op::Setcar | Op::Setcdr => -1,
        Op::Symbolp
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Integerp
        | Op::Numberp
        | Op::Null
        | Op::Not => 0,
        Op::Eq | Op::Equal => -1,
        Op::Concat(n) => -(*n as i32) + 1,
        Op::Substring | Op::StringEqual | Op::StringLessp => -1,
        Op::Aref => -1,
        Op::Aset => -2,
        Op::SymbolValue | Op::SymbolFunction => 0,
        Op::Set | Op::Fset | Op::Get => -1,
        Op::Put => -2,
        Op::PushConditionCase(_) => 0,
        Op::PopHandler => 0,
        Op::UnwindProtect(_) => 0,
        Op::Throw => -1,
        Op::MakeClosure(_) => 1,
        Op::CallBuiltin(_, n) => -(*n as i32) + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::parse_forms;

    fn compile(src: &str) -> ByteCodeFunction {
        let forms = parse_forms(src).expect("parse");
        let mut compiler = Compiler::new(false);
        compiler.compile_toplevel(&forms[0])
    }

    #[test]
    fn compile_literal_int() {
        let func = compile("42");
        assert_eq!(func.ops.len(), 2); // Constant + Return
        assert!(matches!(func.ops[0], Op::Constant(0)));
        assert!(matches!(func.ops[1], Op::Return));
        assert_eq!(func.constants[0].as_int(), Some(42));
    }

    #[test]
    fn compile_nil_t() {
        let func = compile("nil");
        assert!(matches!(func.ops[0], Op::Nil));

        let func = compile("t");
        assert!(matches!(func.ops[0], Op::True));
    }

    #[test]
    fn compile_addition() {
        let func = compile("(+ 1 2)");
        // Constant(1), Constant(2), Add, Return
        assert_eq!(func.ops.len(), 4);
        assert!(matches!(func.ops[2], Op::Add));
    }

    #[test]
    fn compile_if() {
        let func = compile("(if t 1 2)");
        // Has GotoIfNil and Goto for branching
        let has_goto_nil = func.ops.iter().any(|op| matches!(op, Op::GotoIfNil(_)));
        assert!(has_goto_nil);
    }

    #[test]
    fn compile_let() {
        let func = compile("(let ((x 1)) x)");
        let has_varbind = func.ops.iter().any(|op| matches!(op, Op::VarBind(_)));
        let has_unbind = func.ops.iter().any(|op| matches!(op, Op::Unbind(_)));
        assert!(has_varbind);
        assert!(has_unbind);
    }

    #[test]
    fn compile_setq() {
        let func = compile("(setq x 42)");
        let has_varset = func.ops.iter().any(|op| matches!(op, Op::VarSet(_)));
        assert!(has_varset);
    }

    #[test]
    fn compile_while() {
        let func = compile("(while nil 1)");
        let has_goto = func.ops.iter().any(|op| matches!(op, Op::Goto(_)));
        let has_goto_nil = func.ops.iter().any(|op| matches!(op, Op::GotoIfNil(_)));
        assert!(has_goto);
        assert!(has_goto_nil);
    }

    #[test]
    fn compile_lambda() {
        let func = compile("(lambda (x) (+ x 1))");
        // Should have MakeClosure or Constant (depends on lexical mode)
        let has_constant = func.ops.iter().any(|op| matches!(op, Op::Constant(_)));
        assert!(has_constant);
    }

    #[test]
    fn compile_quote() {
        let func = compile("'(1 2 3)");
        assert_eq!(func.ops.len(), 2); // Constant + Return
    }

    #[test]
    fn compile_and_or() {
        let func = compile("(and 1 2 3)");
        let has_short_circuit = func
            .ops
            .iter()
            .any(|op| matches!(op, Op::GotoIfNilElsePop(_)));
        assert!(has_short_circuit);

        let func = compile("(or 1 2 3)");
        let has_short_circuit = func
            .ops
            .iter()
            .any(|op| matches!(op, Op::GotoIfNotNilElsePop(_)));
        assert!(has_short_circuit);
    }

    #[test]
    fn compile_type_predicates() {
        let func = compile("(null x)");
        let has_not = func.ops.iter().any(|op| matches!(op, Op::Not));
        assert!(has_not);

        let func = compile("(consp x)");
        let has_consp = func.ops.iter().any(|op| matches!(op, Op::Consp));
        assert!(has_consp);
    }

    #[test]
    fn compile_list_ops() {
        let func = compile("(car x)");
        assert!(func.ops.iter().any(|op| matches!(op, Op::Car)));

        let func = compile("(cdr x)");
        assert!(func.ops.iter().any(|op| matches!(op, Op::Cdr)));

        let func = compile("(cons 1 2)");
        assert!(func.ops.iter().any(|op| matches!(op, Op::Cons)));
    }

    #[test]
    fn compile_progn() {
        let func = compile("(progn 1 2 3)");
        // Should only keep last value
        assert!(matches!(func.ops.last(), Some(Op::Return)));
    }

    #[test]
    fn disassemble_output() {
        let func = compile("(+ 1 2)");
        let dis = func.disassemble();
        assert!(dis.contains("add"));
        assert!(dis.contains("constant"));
    }
}
