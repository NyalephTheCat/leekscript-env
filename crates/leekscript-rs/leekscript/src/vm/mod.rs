//! Table-driven bytecode VM for LeekScript.
//!
//! The reference implementation under `leek-wars-generator/leekscript` compiles LeekScript to Java
//! source and then to JVM bytecode. There is no single explicit **opcode → handler** map in that
//! pipeline—the host JVM provides dispatch.
//!
//! This module is the start of a self-contained runtime for tools such as a fight generator:
//!
//! - [`Opcode`] — fixed `u8` instruction tags and operand layouts.
//! - [`DISPATCH`](interpreter::DISPATCH) — a full **256-entry** function-pointer table (one handler
//!   per possible opcode byte). Unused tags still resolve to [`op_illegal`](interpreter::op_illegal).
//! - [`Vm`](interpreter::Vm) — stack machine, constant pool, locals, optional [`NativeFn`](interpreter::NativeFn) table.
//! - [`compile_chunk_v4`](compile::compile_chunk_v4) — CST → bytecode (`var`, `if`, `while` / `do`-`while` /
//!   `for`, `break` / `continue`, `;`, `a[i]` / `m.field`, ternary `?:`, `+=` / `-=` / …, simple
//!   `x = expr`, expressions, `return`). Loops also emit [`Opcode::ChargeOps`](Opcode::ChargeOps) to
//!   mirror Java `AI.ops` / `addCounter` at headers (see `compile` module docs).

mod bytecode;
mod compile;
mod error;
mod interpreter;
mod java_ops;
mod opcode;
mod value;

pub use bytecode::{Bytecode, BytecodeBuilder};
pub use compile::{CompileError, CompiledChunk, compile_chunk_v4};
pub use error::VmError;
pub use interpreter::{
    NativeFn, OpHandler, Vm, DEFAULT_MAX_OPERATIONS, DEFAULT_MAX_RAM_QUADS, DISPATCH, op_illegal,
};
pub use opcode::Opcode;
pub use value::Value;
