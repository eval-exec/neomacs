//! Bytecode virtual machine and decoder.
//!
//! Provides:
//! - `opcode::Op` — bytecode instruction set
//! - `chunk::ByteCodeFunction` — compiled function representation
//! - `vm::Vm` — stack-based bytecode interpreter
//! - `decode` — GNU .elc bytecode decoder

pub mod chunk;
pub mod decode;
pub mod opcode;
pub mod vm;

// Re-export main types
pub(crate) use chunk::fresh_bytecode_source_id;
pub use chunk::{ByteCodeFunction, ByteCodeSlot};
pub use opcode::Op;
pub use vm::Vm;
