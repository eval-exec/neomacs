//! Bytecode virtual machine and decoder.
//!
//! Provides:
//! - `opcode::Op` — bytecode instruction set
//! - `chunk::ByteCodeFunction` — compiled function representation
//! - `vm::Vm` — stack-based bytecode interpreter
//! - `decode` — GNU .elc bytecode decoder
//!
//! Runtime knobs (read once per process):
//! | Knob | Default | Meaning |
//! |---|---|---|
//! | `NEOVM_VM_STACK_RETURN` | off; `on`/`1`/`true`/`yes` enables | Return interpreter stack headers to the Context-owned pool before clearing their destination vectors, avoiding a wide reload after partial header stores. No generated CLIF change; promote after the callback-seam performance gates pass, or delete. |

pub(crate) mod arith_kind;
pub mod chunk;
pub mod decode;
pub mod opcode;
pub mod vm;

// Re-export main types
#[cfg(any(feature = "jit", test))]
pub(crate) use arith_kind::ArithGenericKind;
pub(crate) use chunk::fresh_bytecode_source_id;
pub use chunk::{ByteCodeFunction, ByteCodeStructuralPart};
pub use opcode::Op;
pub use vm::Vm;
