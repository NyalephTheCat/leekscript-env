//! CST → bytecode lowering ([`compile_chunk_v4`] and helpers).

mod lower;

pub use lower::{
    CompileChunkError, CompileError, CompiledChunk, FunctionEntry, compile_chunk_v4,
    compile_chunk_v4_with_includes, compile_chunk_v4_with_includes_and_native_id_fn,
    compile_chunk_v4_with_native_id_fn,
};
