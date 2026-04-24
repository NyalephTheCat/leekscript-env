//! Control-flow results from executing statements.

use super::value::Value;

pub(super) enum StmtFlow {
    Continue,
    Return(Option<Value>),
    Break,
    ContinueLoop,
    /// Uncaught from `throw` until a `try`/`catch` handles it.
    Throw(Option<Value>),
}
