//! Byte buffers and constant pool produced by the compiler.

use std::vec::Vec;

use super::opcode::Opcode;
use super::value::{PreludeClass, Value};

/// Executable program: raw opcode stream plus constant pool.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bytecode {
    pub code: Vec<u8>,
    pub constants: Vec<Value>,
}

impl Bytecode {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Builds [`Bytecode`] with typed emit helpers.
#[derive(Debug, Default)]
pub struct BytecodeBuilder {
    code: Vec<u8>,
    constants: Vec<Value>,
}

impl BytecodeBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern_const(&mut self, v: Value) -> u32 {
        let idx = self.constants.len();
        self.constants.push(v);
        idx as u32
    }

    pub fn emit_raw(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }

    pub fn emit_opcode(&mut self, op: Opcode) {
        self.code.push(op as u8);
    }

    pub fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }

    pub fn emit_array_build(&mut self, element_count: u16) {
        self.emit_opcode(Opcode::ArrayBuild);
        self.emit_u16_operand(element_count);
    }

    pub fn emit_map_build(&mut self, pair_count: u16) {
        self.emit_opcode(Opcode::MapBuild);
        self.emit_u16_operand(pair_count);
    }

    pub fn emit_object_build(&mut self, pair_count: u16) {
        self.emit_opcode(Opcode::ObjectBuild);
        self.emit_u16_operand(pair_count);
    }

    pub fn emit_array_len(&mut self) {
        self.emit_opcode(Opcode::ArrayLen);
    }

    pub fn emit_map_len(&mut self) {
        self.emit_opcode(Opcode::MapLen);
    }

    pub fn emit_map_entry_at(&mut self) {
        self.emit_opcode(Opcode::MapEntryAt);
    }

    pub fn emit_call_function(&mut self, func_id: u16, argc: u8) {
        self.emit_opcode(Opcode::CallFunction);
        self.emit_u16_operand(func_id);
        self.emit_u8(argc);
    }

    /// [`Opcode::TryBegin`](Opcode::TryBegin) with placeholder `u32` catch PC; patch via [`Self::patch_u32_at`].
    pub fn emit_try_begin_placeholder(&mut self) -> usize {
        self.emit_opcode(Opcode::TryBegin);
        let off = self.code.len();
        self.code.extend_from_slice(&0u32.to_le_bytes());
        off
    }

    pub fn emit_try_end(&mut self) {
        self.emit_opcode(Opcode::TryEnd);
    }

    pub fn emit_throw(&mut self) {
        self.emit_opcode(Opcode::Throw);
    }

    pub fn emit_u16_operand(&mut self, v: u16) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    pub fn emit_i32_operand(&mut self, v: i32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// [`Opcode::ChargeOps`](Opcode::ChargeOps) followed by little-endian `u32`.
    pub fn emit_charge_ops(&mut self, n: u32) {
        self.emit_opcode(Opcode::ChargeOps);
        self.code.extend_from_slice(&n.to_le_bytes());
    }

    pub fn emit_push_const(&mut self, v: Value) {
        let idx = self.intern_const(v);
        self.emit_opcode(Opcode::PushConst);
        self.code.extend_from_slice(&idx.to_le_bytes());
    }

    /// Prelude `Array` / `Null` globals: no constant pool entry (see [`Opcode::PushPreludeClass`](Opcode::PushPreludeClass)).
    pub fn emit_push_prelude_class(&mut self, c: PreludeClass) {
        self.emit_opcode(Opcode::PushPreludeClass);
        self.emit_u8(c.to_u8());
    }

    pub fn emit_return(&mut self) {
        self.emit_opcode(Opcode::Return);
    }

    /// [`Opcode::JumpIfFalse`] with a placeholder `i32` delta; returns the **byte offset** of the
    /// operand (patch with [`Self::patch_i32_operand_at`]).
    pub fn emit_jump_if_false_placeholder(&mut self) -> usize {
        self.emit_opcode(Opcode::JumpIfFalse);
        let off = self.code.len();
        self.code.extend_from_slice(&0i32.to_le_bytes());
        off
    }

    /// [`Opcode::Jump`] with a placeholder `i32` delta; returns operand byte offset.
    pub fn emit_jump_placeholder(&mut self) -> usize {
        self.emit_opcode(Opcode::Jump);
        let off = self.code.len();
        self.code.extend_from_slice(&0i32.to_le_bytes());
        off
    }

    pub fn patch_i32_operand_at(&mut self, operand_offset: usize, delta: i32) {
        self.code[operand_offset..operand_offset + 4].copy_from_slice(&delta.to_le_bytes());
    }

    pub fn emit_call_native(&mut self, native_id: u16, argc: u8) {
        self.emit_opcode(Opcode::CallNative);
        self.emit_u16_operand(native_id);
        self.code.push(argc);
    }

    pub fn patch_u32_at(&mut self, offset: usize, word: u32) {
        let b = word.to_le_bytes();
        self.code[offset..offset + 4].copy_from_slice(&b);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.code.len()
    }

    #[must_use]
    pub fn finish(self) -> Bytecode {
        Bytecode {
            code: self.code,
            constants: self.constants,
        }
    }
}
