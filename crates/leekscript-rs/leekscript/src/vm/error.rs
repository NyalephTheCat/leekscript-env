//! Runtime errors for the LeekScript bytecode VM.

use core::fmt;

/// Failure while executing [`super::Vm`](super::Vm).
#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    /// Program counter moved past the end of the code buffer without `Return`.
    UnexpectedEof,
    /// Unknown or unassigned opcode byte (dispatch table gap).
    IllegalOpcode(u8),
    /// Constant pool index out of range.
    BadConstantIndex(u32),
    /// Stack underflow (too few values for an operation).
    StackUnderflow,
    /// Division where the divisor rounded to zero.
    DivByZero,
    /// Native function id out of range.
    BadNativeIndex(u16),
    /// Wrong number of arguments for a native call.
    BadArgCount { expected: u8, got: usize },
    /// Operand was not a number where arithmetic expected one.
    ExpectedNumber,
    /// Operand was not a string where a string was required.
    ExpectedString,
    /// Operand was not an array where an array was required.
    ExpectedArray,
    /// Operand was not an interval where an interval was required.
    ExpectedInterval,
    /// Native overload / argument types not supported at runtime.
    BadNativeArgs,
    /// Local slot index out of range for the current frame.
    BadLocal(u16),
    /// [`Vm::max_operations`](super::Vm::max_operations) exceeded (Leek Wars `Error.TOO_MUCH_OPERATIONS`).
    TooManyOperations { limit: u64, attempted_total: u64 },
    /// [`Vm::max_ram_quads`](super::Vm::max_ram_quads) exceeded (Leek Wars `Error.OUT_OF_MEMORY`).
    OutOfMemory { limit: u64, attempted_total: u64 },
    /// [`Opcode::CallFunction`](super::opcode::Opcode::CallFunction) id out of range.
    BadFunctionIndex(u16),
    /// Call arity does not match the compiled function.
    BadFunctionArity { expected: u8, got: u8 },
    /// Attempted to call a non-callable value.
    BadValueCall(super::value::Value),
    /// [`Opcode::Throw`](super::opcode::Opcode::Throw) with no enclosing [`Opcode::TryBegin`](super::opcode::Opcode::TryBegin).
    UncaughtThrow(super::value::Value),
    /// [`Opcode::TryEnd`](super::opcode::Opcode::TryEnd) without a matching [`Opcode::TryBegin`](super::opcode::Opcode::TryBegin).
    TryStackUnderflow,
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of bytecode"),
            Self::IllegalOpcode(b) => write!(f, "illegal opcode {b}"),
            Self::BadConstantIndex(i) => write!(f, "constant index {i} out of range"),
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::DivByZero => write!(f, "division by zero"),
            Self::BadNativeIndex(i) => write!(f, "native function index {i} out of range"),
            Self::BadArgCount { expected, got } => {
                write!(f, "native call expected {expected} args, got {got}")
            }
            Self::ExpectedNumber => write!(f, "expected number"),
            Self::ExpectedString => write!(f, "expected string"),
            Self::ExpectedArray => write!(f, "expected array"),
            Self::ExpectedInterval => write!(f, "expected interval"),
            Self::BadNativeArgs => write!(f, "native call argument types not supported"),
            Self::BadLocal(i) => write!(f, "local index {i} out of range"),
            Self::TooManyOperations {
                limit,
                attempted_total,
            } => write!(
                f,
                "operation limit exceeded (limit {limit}, would be {attempted_total})"
            ),
            Self::OutOfMemory {
                limit,
                attempted_total,
            } => write!(
                f,
                "RAM limit exceeded (limit {limit} quads, would be {attempted_total})"
            ),
            Self::BadFunctionIndex(i) => write!(f, "function index {i} out of range"),
            Self::BadFunctionArity { expected, got } => {
                write!(f, "function call expected {expected} args, got {got}")
            }
            Self::BadValueCall(v) => write!(f, "attempted to call non-function value {v:?}"),
            Self::UncaughtThrow(_) => write!(f, "uncaught throw"),
            Self::TryStackUnderflow => write!(f, "`TryEnd` without matching `TryBegin`"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VmError {}
