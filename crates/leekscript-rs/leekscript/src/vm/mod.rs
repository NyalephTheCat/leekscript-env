//! Table-driven bytecode VM for LeekScript.
//!
//! # Layout
//!
//! Submodules group responsibilities; the crate root re-exports the common entry points so
//! `leekscript::vm::{Vm, compile_chunk_v4, …}` stays stable.
//!
//! | Module | Role |
//! |--------|------|
//! | [`ir`] | Opcode tags and [`Bytecode`] / builder (what the compiler emits). |
//! | [`value`] | Runtime [`Value`], [`NumberBits`], prelude class tags. |
//! | [`runtime`] | [`Vm`], [`DISPATCH`](runtime::interpreter::DISPATCH), [`VmError`], stdlib natives. |
//! | [`compile`] | CST → bytecode ([`compile_chunk_v4`], …). |
//! | [`host`] | Java-style static op counts ([`host::java_ops`]) and JSON helpers ([`host::json`]). |
//!
//! The reference implementation under `leek-wars-generator/leekscript` compiles LeekScript to Java
//! source and then to JVM bytecode. There is no single explicit **opcode → handler** map in that
//! pipeline—the host JVM provides dispatch.
//!
//! This module is a self-contained runtime for tools such as a fight generator:
//!
//! - [`Opcode`] — fixed `u8` instruction tags and operand layouts.
//! - [`DISPATCH`](runtime::interpreter::DISPATCH) — a full **256-entry** function-pointer table (one handler
//!   per possible opcode byte). Unused tags still resolve to [`op_illegal`](runtime::interpreter::op_illegal).
//! - [`Vm`](runtime::interpreter::Vm) — stack machine, constant pool, locals, [`NativeFn`](runtime::interpreter::NativeFn) table
//!   (defaults from [`default_natives`](runtime::stdlib::default_natives) when using [`Vm::from_compiled_chunk`](runtime::interpreter::Vm::from_compiled_chunk)).
//! - [`stdlib_global_constant_init`](runtime::stdlib::stdlib_global_constant_init) — `PI`, `TYPE_*`, `SORT_*`, `COLOR_*`, …
//!   from `sig/core/stdlib.sig.const.leek`, bound at chunk start.
//! - [`compile_chunk_v4`](compile::compile_chunk_v4) — CST → bytecode (`var` / `global` / `const`, `if`,
//!   `while` / `do`-`while` / `for` / `for (x in arr)`, `break` / `continue`, `;`, `a[i]` / `m.field`,
//!   ternary `?:`, `+=` / `-=` / …, simple `x = expr`, expressions, `return`). Loops emit
//!   [`Opcode::ChargeOps`](Opcode::ChargeOps) to mirror Java `AI.ops` / `addCounter` at headers (see
//!   the [`compile`] module).

pub mod compile;
pub mod host;
pub mod ir;
pub mod runtime;
pub mod value;

pub use compile::{
    CompileChunkError, CompileError, CompiledChunk, FunctionEntry, compile_chunk_v4,
    compile_chunk_v4_with_includes, compile_chunk_v4_with_includes_and_native_id_fn,
    compile_chunk_v4_with_native_id_fn,
};
pub use ir::{Bytecode, BytecodeBuilder, Opcode};
pub use runtime::error::VmError;
pub use runtime::interpreter::{
    DEFAULT_MAX_OPERATIONS, DEFAULT_MAX_RAM_QUADS, DISPATCH, NativeFn, OpHandler, Vm, op_illegal,
};
/// Alias of [`runtime::stdlib`] so `leekscript::vm::stdlib::…` paths stay stable.
pub use runtime::stdlib;
pub use runtime::stdlib::{stdlib_global_constant_init, stdlib_global_function_init};
pub use value::{NumberBits, PreludeClass, Value};
