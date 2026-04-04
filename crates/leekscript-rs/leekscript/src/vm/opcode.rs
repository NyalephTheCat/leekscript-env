//! Bytecode opcode tags (u8 discriminants). Operands are laid out immediately after the tag in
//! little-endian order; see the `vm` module documentation on `leekscript::vm`.

/// Instruction tag. Gaps in numeric space are reserved; unknown bytes dispatch to the illegal handler.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    /// Unassigned opcode byte (also the default fill in [`super::DISPATCH`](super::DISPATCH)).
    Illegal = 0,
    Nop = 1,
    /// Followed by `u32` constant pool index.
    PushConst = 2,
    PushNull = 3,
    Pop = 4,
    Dup = 5,
    Add = 6,
    Sub = 7,
    Mul = 8,
    Div = 9,
    Mod = 10,
    Neg = 11,
    /// Sets the program counter past the end of the code buffer; the return value is the top of stack.
    Return = 12,
    /// Followed by `u16` local index.
    GetLocal = 13,
    /// Followed by `u16` local index.
    SetLocal = 14,
    /// Followed by `i32` delta applied to PC after consuming the operand (branch-if-taken style).
    Jump = 15,
    /// Pop one value; if falsey, add `i32` delta to PC (operand follows opcode).
    JumpIfFalse = 16,
    /// `u16` native id, `u8` argument count. Arguments are popped in stack order (last pushed = last arg).
    CallNative = 17,
    /// V4 `==` / `===` on stack: pop rhs, lhs; push bool (`equals_equals` subset).
    EqEquals = 18,
    /// V4 `!=` / `!==` on stack.
    NeEquals = 19,
    /// Pop rhs, lhs; push bool (`real(lhs) < real(rhs)`).
    Lt = 20,
    Lte = 21,
    Gt = 22,
    Gte = 23,
    /// Pop value; push bool (`!truthy`).
    Not = 24,
    /// Pop `n` values (last pushed = last element); push one [`Value::Array`](super::value::Value::Array). Followed by `u8` element count `n` (max 255).
    ArrayBuild = 25,
}
