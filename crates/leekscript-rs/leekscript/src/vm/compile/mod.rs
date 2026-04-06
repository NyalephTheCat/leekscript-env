//! CST → bytecode lowering ([`compile_chunk_v4`] and helpers).

mod lower;

pub use lower::{
    CompileChunkError, CompileError, CompiledChunk, FunctionEntry, compile_chunk_v4,
    compile_chunk_v4_with_includes,
};
