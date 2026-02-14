//! Compatibility helpers for Emacs compiled-function reader literals (`#[...]`, `#(...)`).

use super::bytecode::{ByteCodeFunction, Op};
use super::error::{signal, Flow};
use super::value::{list_to_vec, LambdaParams, Value};

/// Convert parsed Emacs compiled-function literal vectors into typed
/// `Value::ByteCode` functions.
///
/// For now we decode a compatibility subset of GNU Emacs bytecode opcodes.
/// Unknown opcode streams still coerce to an explicit placeholder that raises.
pub(crate) fn maybe_coerce_compiled_literal_function(value: Value) -> Value {
    let Value::Vector(items_ref) = &value else {
        return value;
    };
    let Some(bytecode) = compiled_literal_vector_to_bytecode(items_ref) else {
        return value;
    };
    Value::ByteCode(std::sync::Arc::new(bytecode))
}

/// Build a typed placeholder from a `(byte-code BYTESTR CONSTS MAXDEPTH ...)`
/// form used by some `.elc` payloads.
pub(crate) fn placeholder_from_byte_code_form(args: &[Value]) -> Result<Value, Flow> {
    if args.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("byte-code"), Value::Int(args.len() as i64)],
        ));
    }

    if !matches!(args[0], Value::Str(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), args[0].clone()],
        ));
    }
    if !matches!(args[1], Value::Vector(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("vectorp"), args[1].clone()],
        ));
    }
    let max_depth = match args[2] {
        Value::Int(n) if (0..=u16::MAX as i64).contains(&n) => Value::Int(n),
        Value::Int(_) => {
            return Err(signal("args-out-of-range", vec![args[2].clone()]));
        }
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integerp"), args[2].clone()],
            ));
        }
    };

    let params = if let Some(value) = args.get(3) {
        if value.is_nil() || list_to_vec(value).is_some() {
            value.clone()
        } else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("listp"), value.clone()],
            ));
        }
    } else {
        Value::Nil
    };

    let mut items = vec![params, args[0].clone(), args[1].clone(), max_depth];
    if let Some(extra) = args.get(4) {
        items.push(extra.clone());
    }

    let coerced = maybe_coerce_compiled_literal_function(Value::vector(items));
    if matches!(coerced, Value::ByteCode(_)) {
        Ok(coerced)
    } else {
        Err(signal(
            "error",
            vec![Value::string("Invalid byte-code object")],
        ))
    }
}

fn compiled_literal_vector_to_bytecode(
    items_ref: &std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
) -> Option<ByteCodeFunction> {
    let items = items_ref.lock().ok()?;
    if items.len() < 4 {
        return None;
    }

    let params = parse_compiled_literal_params(&items[0])?;
    if items[1].as_str().is_none() {
        return None;
    }
    let Value::Vector(constants_ref) = &items[2] else {
        return None;
    };
    let max_stack = match items[3] {
        Value::Int(n) if (0..=u16::MAX as i64).contains(&n) => n as u16,
        _ => return None,
    };

    let byte_stream = items[1].as_str()?;
    let mut bytecode = ByteCodeFunction::new(params);
    bytecode.max_stack = max_stack;
    bytecode.constants = constants_ref.lock().ok()?.clone();
    if let Some(Value::Str(s)) = items.get(4) {
        bytecode.docstring = Some((**s).clone());
    }

    if let Some(decoded) = decode_opcode_subset(byte_stream, bytecode.constants.len()) {
        bytecode.ops = decoded;
    } else {
        let idx = bytecode.add_symbol("%%unimplemented-elc-bytecode");
        bytecode.emit(Op::CallBuiltin(idx, 0));
        bytecode.emit(Op::Return);
    }
    Some(bytecode)
}

fn decode_opcode_subset(byte_stream: &str, const_len: usize) -> Option<Vec<Op>> {
    let bytes = decode_unibyte_stream(byte_stream)?;
    let mut ops = Vec::with_capacity(bytes.len());
    for b in bytes {
        match b {
            // byte-constant 0..63
            0o300..=0o377 => {
                let idx = (b - 0o300) as usize;
                if idx >= const_len {
                    return None;
                }
                ops.push(Op::Constant(idx as u16));
            }
            // varref 0..7
            0o010..=0o017 => {
                let idx = (b - 0o010) as usize;
                if idx >= const_len {
                    return None;
                }
                ops.push(Op::VarRef(idx as u16));
            }
            // return
            0o207 => ops.push(Op::Return),
            _ => return None,
        }
    }

    if ops.is_empty() {
        return None;
    }
    Some(ops)
}

fn decode_unibyte_stream(byte_stream: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(byte_stream.len());
    for ch in byte_stream.chars() {
        let code = ch as u32;
        if code > u8::MAX as u32 {
            return None;
        }
        out.push(code as u8);
    }
    Some(out)
}

fn parse_compiled_literal_params(value: &Value) -> Option<LambdaParams> {
    if value.is_nil() {
        return Some(LambdaParams::simple(vec![]));
    }
    let items = list_to_vec(value)?;
    let mut required = Vec::new();
    let mut optional = Vec::new();
    let mut rest = None;
    let mut mode = 0_u8; // 0 = required, 1 = optional, 2 = rest

    for item in items {
        let name = item.as_symbol_name()?;
        match name {
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
            0 => required.push(name.to_string()),
            1 => optional.push(name.to_string()),
            2 => {
                rest = Some(name.to_string());
                break;
            }
            _ => unreachable!(),
        }
    }

    Some(LambdaParams {
        required,
        optional,
        rest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_vector_passthrough() {
        let v = Value::Int(42);
        assert_eq!(maybe_coerce_compiled_literal_function(v.clone()), v);
    }

    #[test]
    fn invalid_vector_passthrough() {
        let v = Value::vector(vec![Value::Int(1), Value::Int(2)]);
        assert!(matches!(
            maybe_coerce_compiled_literal_function(v),
            Value::Vector(_)
        ));
    }

    #[test]
    fn coerces_compiled_literal_vector_to_placeholder_bytecode() {
        let literal = Value::vector(vec![
            Value::list(vec![
                Value::symbol("x"),
                Value::symbol("&optional"),
                Value::symbol("y"),
                Value::symbol("&rest"),
                Value::symbol("rest"),
            ]),
            Value::string("\u{8}T\u{87}"),
            Value::vector(vec![Value::symbol("x"), Value::Int(7)]),
            Value::Int(3),
            Value::string("doc"),
        ]);

        let coerced = maybe_coerce_compiled_literal_function(literal);
        let Value::ByteCode(bc) = coerced else {
            panic!("expected Value::ByteCode");
        };

        assert_eq!(bc.params.required, vec!["x"]);
        assert_eq!(bc.params.optional, vec!["y"]);
        assert_eq!(bc.params.rest.as_deref(), Some("rest"));
        assert_eq!(bc.max_stack, 3);
        assert_eq!(bc.constants[0], Value::symbol("x"));
        assert_eq!(bc.constants[1], Value::Int(7));
        assert_eq!(bc.docstring.as_deref(), Some("doc"));
        match bc.ops.as_slice() {
            [Op::CallBuiltin(idx, 0), Op::Return] => {
                assert_eq!(
                    bc.constants[*idx as usize],
                    Value::symbol("%%unimplemented-elc-bytecode")
                );
            }
            other => panic!("unexpected placeholder ops: {other:?}"),
        }
    }

    #[test]
    fn decodes_constant_return_opcode_subset() {
        let literal = Value::vector(vec![
            Value::Nil,
            Value::string("\u{C0}\u{87}"),
            Value::vector(vec![Value::Int(42)]),
            Value::Int(1),
        ]);
        let coerced = maybe_coerce_compiled_literal_function(literal);
        let Value::ByteCode(bc) = coerced else {
            panic!("expected Value::ByteCode");
        };
        assert_eq!(bc.ops, vec![Op::Constant(0), Op::Return]);
    }

    #[test]
    fn decodes_varref_return_opcode_subset() {
        let literal = Value::vector(vec![
            Value::list(vec![Value::symbol("x")]),
            Value::string("\u{8}\u{87}"),
            Value::vector(vec![Value::symbol("x")]),
            Value::Int(1),
        ]);
        let coerced = maybe_coerce_compiled_literal_function(literal);
        let Value::ByteCode(bc) = coerced else {
            panic!("expected Value::ByteCode");
        };
        assert_eq!(bc.ops, vec![Op::VarRef(0), Op::Return]);
    }

    #[test]
    fn byte_code_form_coerces_to_placeholder() {
        let value = placeholder_from_byte_code_form(&[
            Value::string("\u{8}T\u{87}"),
            Value::vector(vec![Value::symbol("x")]),
            Value::Int(1),
        ])
        .expect("byte-code form should coerce");
        assert!(matches!(value, Value::ByteCode(_)));
    }

    #[test]
    fn byte_code_form_rejects_non_vector_constants() {
        let err = placeholder_from_byte_code_form(&[
            Value::string("\u{8}T\u{87}"),
            Value::Int(1),
            Value::Int(1),
        ])
        .expect_err("non-vector constants should fail");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument")
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
