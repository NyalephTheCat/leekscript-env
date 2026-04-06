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
    /// Pop `n` values (last pushed = last element); push one [`Value::Array`](super::value::Value::Array). Followed by `u16` element count `n`.
    ArrayBuild = 25,
    /// Pop `n` key/value pairs (last pushed = last value); push [`Value::Map`](super::value::Value::Map) in source order. Followed by `u16` pair count `n`.
    MapBuild = 26,
    /// Pop **key**, then **container**; push element (`null` if out of range / missing key / bad container).
    GetElem = 27,
    /// Add `u32` to the operation budget (Java `AI.ops(int)` at control-flow boundaries). No `+1`
    /// dispatch tick — the operand is the full semantic charge for this instruction.
    ChargeOps = 28,
    /// Pop one value; push its array length as a number (`0` if not an array).
    ArrayLen = 29,
    /// Pop map; push pair count as a number (`0` if not a map).
    MapLen = 30,
    /// Pop index then map; push key then value for that pair (`null`, `null` if out of range / not a map).
    MapEntryAt = 31,
    /// Call user function: `u16` function id, `u8` argc. Pops args (last pushed = last param).
    CallFunction = 32,
    /// Push active `try` handler: `u32` absolute bytecode offset of the `catch` entry.
    TryBegin = 33,
    /// Pop one `try` frame (normal completion of `try` body).
    TryEnd = 34,
    /// Pop thrown value; jump to innermost `catch` with that value pushed, or error if none.
    Throw = 35,
    /// Pop rhs, lhs; push bool (`xor(truthy(lhs), truthy(rhs))` like Java `AI.xor`).
    LogicalXor = 36,
    /// Like [`MapBuild`](Self::MapBuild) but pushes [`Value::Object`](super::value::Value::Object).
    ObjectBuild = 37,
    /// Truncating integer division (`\`), Java-style toward zero.
    IntDiv = 38,
    /// Prelude class binding (`Array`, `Null`): `u8` [`super::value::PreludeClass`](super::value::PreludeClass) discriminant (not from constant pool).
    PushPreludeClass = 39,
    /// Pop **rhs**, then **key**; assign into local `slot`’s array/map (`null` base → push `null`, slot unchanged).
    /// Pushes the assignment expression value (`null` or `rhs`).
    SetElemLocal = 40,
    /// Pop `n` values (last pushed = last source element); push [`Value::Set`](super::value::Value::Set) (sorted, deduped). Followed by `u16` `n`.
    SetBuild = 41,
    /// Pop element; read set from local `u16`; update local; push bool (added).
    SetPutLocal = 42,
    /// Pop element; read set from local `u16`; update local; push bool (removed).
    SetRemoveLocal = 43,
    /// Clear set at local `u16`; push empty set (discarded by stmt) — leaves `null` on stack for value.
    SetClearLocal = 44,
    /// Build an interval value (LeekScript `[..]`, `[1..2[`, `]..1]`, …).
    ///
    /// Followed by `u8` flags:
    /// - bit 0: left closed (`[`). If unset, left is open (`]`).
    /// - bit 1: right closed (`]`). If unset, right is open (`[`).
    /// - bit 2: has left bound value on stack.
    /// - bit 3: has right bound value on stack.
    ///
    /// Stack: optionally pops right, then left (when present); pushes one interval.
    IntervalBuild = 45,
    /// `lhs instanceof Type` where `Type` is a builtin type keyword.
    ///
    /// Followed by `u8` type tag (same numbering as `TYPE_*`): pushes bool.
    /// Pops one value (lhs).
    InstanceofTag = 46,
}
