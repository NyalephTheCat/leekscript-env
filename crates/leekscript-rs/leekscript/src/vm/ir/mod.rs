//! Instruction tags and bytecode buffers: compiler output and interpreter input.

mod bytecode;
mod opcode;

pub use bytecode::{Bytecode, BytecodeBuilder};
pub use opcode::Opcode;
