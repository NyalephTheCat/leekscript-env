//! Stack interpreter and **256-entry opcode dispatch table** (`DISPATCH`).
//!
//! Each entry is a function pointer; unassigned slots point at [`op_illegal`]. This is the
//! execution model the Java reference pipeline does not expose: it emits source/Java and relies on
//! the JVM instead of a single explicit opcode → handler table.

use std::vec::Vec;

use super::bytecode::Bytecode;
use super::compile::FunctionEntry;
use super::error::VmError;
use super::opcode::Opcode;
use super::value::Value;

/// Default operation budget (matches Java `AI.MAX_OPERATIONS`).
pub const DEFAULT_MAX_OPERATIONS: u64 = 20_000_000;
/// Default RAM budget in **quads** (matches Java `AI.MAX_RAM`; multiply by 8 for bytes like `System.getUsedRAM`).
pub const DEFAULT_MAX_RAM_QUADS: u64 = 12_500_000;

/// Host-provided native (`System.debug`, fight APIs, …). Receives arguments in call order (first
/// parameter is `args[0]`). Charge work with [`Vm::add_operations`](Vm::add_operations) to mirror
/// Java `LeekFunctions.getOperations()` and runtime `ai.ops(...)` in `*Class` / `ArrayLeekValue`.
pub type NativeFn = fn(&mut Vm, &[Value]) -> Result<Value, VmError>;

/// One handler per possible `u8` opcode; index `0` is [`Opcode::Illegal`](Opcode::Illegal).
pub type OpHandler = fn(&mut Vm) -> Result<(), VmError>;

/// Opcode → handler. Every index is populated; unknown opcodes at runtime still hit [`op_illegal`].
pub static DISPATCH: [OpHandler; 256] = {
    let mut table: [OpHandler; 256] = [op_illegal; 256];
    table[Opcode::Nop as usize] = op_nop;
    table[Opcode::PushConst as usize] = op_push_const;
    table[Opcode::PushNull as usize] = op_push_null;
    table[Opcode::Pop as usize] = op_pop;
    table[Opcode::Dup as usize] = op_dup;
    table[Opcode::Add as usize] = op_add;
    table[Opcode::Sub as usize] = op_sub;
    table[Opcode::Mul as usize] = op_mul;
    table[Opcode::Div as usize] = op_div;
    table[Opcode::Mod as usize] = op_mod;
    table[Opcode::Neg as usize] = op_neg;
    table[Opcode::Return as usize] = op_return;
    table[Opcode::GetLocal as usize] = op_get_local;
    table[Opcode::SetLocal as usize] = op_set_local;
    table[Opcode::Jump as usize] = op_jump;
    table[Opcode::JumpIfFalse as usize] = op_jump_if_false;
    table[Opcode::CallNative as usize] = op_call_native;
    table[Opcode::EqEquals as usize] = op_eq_equals;
    table[Opcode::NeEquals as usize] = op_ne_equals;
    table[Opcode::Lt as usize] = op_lt;
    table[Opcode::Lte as usize] = op_lte;
    table[Opcode::Gt as usize] = op_gt;
    table[Opcode::Gte as usize] = op_gte;
    table[Opcode::Not as usize] = op_not;
    table[Opcode::ArrayBuild as usize] = op_array_build;
    table[Opcode::MapBuild as usize] = op_map_build;
    table[Opcode::GetElem as usize] = op_get_elem;
    table[Opcode::ChargeOps as usize] = op_charge_ops;
    table[Opcode::ArrayLen as usize] = op_array_len;
    table[Opcode::MapLen as usize] = op_map_len;
    table[Opcode::MapEntryAt as usize] = op_map_entry_at;
    table[Opcode::CallFunction as usize] = op_call_function;
    table[Opcode::TryBegin as usize] = op_try_begin;
    table[Opcode::TryEnd as usize] = op_try_end;
    table[Opcode::Throw as usize] = op_throw;
    table
};

/// LeekScript bytecode interpreter (stack machine).
pub struct Vm {
    code: Vec<u8>,
    constants: Vec<Value>,
    pc: usize,
    stack: Vec<Value>,
    locals: Vec<Value>,
    natives: Vec<NativeFn>,
    /// User functions from [`super::compile::CompiledChunk::functions`](super::compile::CompiledChunk).
    pub functions: Vec<FunctionEntry>,
    return_pcs: Vec<usize>,
    try_catch_pcs: Vec<usize>,
    /// Last opcode byte dispatched (for [`VmError::IllegalOpcode`]).
    pub current_opcode: u8,
    /// Java `AI.mOperations`-style budget: increments from [`Opcode::ChargeOps`](Opcode::ChargeOps) and
    /// from runtime extras that mirror Java (e.g. string/array `+` in [`op_add`](fn@op_add)), not from
    /// a generic per-opcode tick.
    pub operations: u64,
    /// `None` = no limit.
    pub max_operations: Option<u64>,
    /// Live RAM estimate for values on stack + locals (quads; see [`Value::ram_quads`](Value::ram_quads)).
    pub ram_quads: u64,
    /// `None` = no limit.
    pub max_ram_quads: Option<u64>,
}

impl Vm {
    /// Build a VM from [`super::compile::CompiledChunk`](super::compile::CompiledChunk): bytecode,
    /// [`Self::set_local_count`](Self::set_local_count), and [`Self::set_functions`](Self::set_functions).
    pub fn from_compiled_chunk(chunk: super::compile::CompiledChunk) -> Result<Self, VmError> {
        let mut vm = Self::new(chunk.bytecode);
        vm.set_natives(super::stdlib::default_natives());
        vm.set_functions(chunk.functions);
        vm.set_local_count(chunk.local_slots)?;
        Ok(vm)
    }

    /// New VM with empty native table; register callables before `run` if you emit [`Opcode::CallNative`](Opcode::CallNative).
    ///
    /// Initializes [`Self::max_operations`] and [`Self::max_ram_quads`] to the same defaults as Java `AI`.
    #[must_use]
    pub fn new(bytecode: Bytecode) -> Self {
        Self {
            code: bytecode.code,
            constants: bytecode.constants,
            pc: 0,
            stack: Vec::new(),
            locals: Vec::new(),
            natives: Vec::new(),
            functions: Vec::new(),
            return_pcs: Vec::new(),
            try_catch_pcs: Vec::new(),
            current_opcode: 0,
            operations: 0,
            max_operations: Some(DEFAULT_MAX_OPERATIONS),
            ram_quads: 0,
            max_ram_quads: Some(DEFAULT_MAX_RAM_QUADS),
        }
    }

    /// Replace the native function table (index = id used in [`Opcode::CallNative`](Opcode::CallNative)).
    pub fn set_natives(&mut self, natives: Vec<NativeFn>) {
        self.natives = natives;
    }

    /// Install the function table from [`super::compile::compile_chunk_v4`](super::compile::compile_chunk_v4).
    pub fn set_functions(&mut self, functions: Vec<FunctionEntry>) {
        self.functions = functions;
    }

    /// Grow or shrink locals. New slots are [`Value::Null`](Value::Null). Charges/releases RAM per slot.
    pub fn set_local_count(&mut self, n: usize) -> Result<(), VmError> {
        while self.locals.len() < n {
            self.charge_ram(Value::Null.ram_quads())?;
            self.locals.push(Value::Null);
        }
        while self.locals.len() > n {
            let v = self.locals.pop().expect("len > n implies non-empty");
            self.release_ram(v.ram_quads());
        }
        Ok(())
    }

    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    /// Run until [`Opcode::Return`](Opcode::Return) or code end. The returned value is the top of
    /// the stack when `Return` executes (or [`Value::Null`](Value::Null) if the stack was empty).
    pub fn run(&mut self) -> Result<Value, VmError> {
        loop {
            if self.pc >= self.code.len() {
                return Err(VmError::UnexpectedEof);
            }
            let op = self.read_u8()?;
            self.current_opcode = op;
            DISPATCH[op as usize](self)?;
            if op == Opcode::Return as u8 {
                return Ok(self.take_return_value());
            }
        }
    }

    /// [`Self::ram_quads`] × 8 (same scaling as Java `System.getUsedRAM`).
    #[must_use]
    pub fn ram_bytes(&self) -> u64 {
        self.ram_quads.saturating_mul(8)
    }

    /// Reset instruction counter only (does not rewind PC or clear stack).
    pub fn reset_operations_counter(&mut self) {
        self.operations = 0;
    }

    fn take_return_value(&mut self) -> Value {
        match self.stack.pop() {
            Some(v) => {
                self.release_ram(v.ram_quads());
                v
            }
            None => Value::Null,
        }
    }

    /// Add to the instruction counter (Leek Wars `AI.ops` / `addCounter`).
    pub fn add_operations(&mut self, n: u64) -> Result<(), VmError> {
        self.charge_ops(n)
    }

    fn charge_ops(&mut self, n: u64) -> Result<(), VmError> {
        let next = self.operations.saturating_add(n);
        if let Some(limit) = self.max_operations {
            if next > limit {
                return Err(VmError::TooManyOperations {
                    limit,
                    attempted_total: next,
                });
            }
        }
        self.operations = next;
        Ok(())
    }

    fn charge_ram(&mut self, quads: u64) -> Result<(), VmError> {
        let next = self.ram_quads.saturating_add(quads);
        if let Some(limit) = self.max_ram_quads {
            if next > limit {
                return Err(VmError::OutOfMemory {
                    limit,
                    attempted_total: next,
                });
            }
        }
        self.ram_quads = next;
        Ok(())
    }

    fn release_ram(&mut self, quads: u64) {
        self.ram_quads = self.ram_quads.saturating_sub(quads);
    }

    fn push_stack(&mut self, v: Value) -> Result<(), VmError> {
        self.charge_ram(v.ram_quads())?;
        self.stack.push(v);
        Ok(())
    }

    fn pop_stack(&mut self) -> Result<Value, VmError> {
        let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        self.release_ram(v.ram_quads());
        Ok(v)
    }

    fn read_u8(&mut self) -> Result<u8, VmError> {
        let b = *self.code.get(self.pc).ok_or(VmError::UnexpectedEof)?;
        self.pc += 1;
        Ok(b)
    }

    fn read_u16(&mut self) -> Result<u16, VmError> {
        if self.pc + 2 > self.code.len() {
            return Err(VmError::UnexpectedEof);
        }
        let v = u16::from_le_bytes([self.code[self.pc], self.code[self.pc + 1]]);
        self.pc += 2;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32, VmError> {
        if self.pc + 4 > self.code.len() {
            return Err(VmError::UnexpectedEof);
        }
        let v = u32::from_le_bytes([
            self.code[self.pc],
            self.code[self.pc + 1],
            self.code[self.pc + 2],
            self.code[self.pc + 3],
        ]);
        self.pc += 4;
        Ok(v)
    }

    fn read_i32(&mut self) -> Result<i32, VmError> {
        Ok(self.read_u32()? as i32)
    }

}

/// Handler installed for every unused opcode slot and for truly unknown bytes at runtime.
pub fn op_illegal(vm: &mut Vm) -> Result<(), VmError> {
    Err(VmError::IllegalOpcode(vm.current_opcode))
}

fn op_nop(_vm: &mut Vm) -> Result<(), VmError> {
    Ok(())
}

fn op_charge_ops(vm: &mut Vm) -> Result<(), VmError> {
    let n = vm.read_u32()? as u64;
    vm.charge_ops(n)
}

fn op_push_const(vm: &mut Vm) -> Result<(), VmError> {
    let idx = vm.read_u32()? as usize;
    let v = vm
        .constants
        .get(idx)
        .cloned()
        .ok_or(VmError::BadConstantIndex(idx as u32))?;
    vm.push_stack(v)?;
    Ok(())
}

fn op_push_null(vm: &mut Vm) -> Result<(), VmError> {
    vm.push_stack(Value::Null)?;
    Ok(())
}

fn op_pop(vm: &mut Vm) -> Result<(), VmError> {
    vm.pop_stack()?;
    Ok(())
}

fn op_dup(vm: &mut Vm) -> Result<(), VmError> {
    let v = vm.pop_stack()?;
    vm.push_stack(v.clone())?;
    vm.push_stack(v)?;
    Ok(())
}

fn op_add(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    let out = match (&a, &b) {
        (Value::String(_), _) | (_, Value::String(_)) => {
            let sa = a.to_leek_coerce_string();
            let sb = b.to_leek_coerce_string();
            let extra = (sa.len() + sb.len()) as u64;
            vm.charge_ops(extra)?;
            Value::String(format!("{sa}{sb}"))
        }
        (Value::Array(ax), Value::Array(bx)) => {
            let extra = 2u64.saturating_add(ax.len() as u64).saturating_add(bx.len() as u64);
            vm.charge_ops(extra)?;
            let mut v = ax.clone();
            v.extend(bx.iter().cloned());
            Value::Array(v)
        }
        (Value::Array(ax), _) => {
            vm.charge_ops((ax.len() as u64).saturating_mul(2))?;
            let mut v = ax.clone();
            v.push(b);
            Value::Array(v)
        }
        (_, Value::Array(bx)) => {
            vm.charge_ops((bx.len() as u64).saturating_mul(2))?;
            let mut v = Vec::with_capacity(1 + bx.len());
            v.push(a);
            v.extend(bx.iter().cloned());
            Value::Array(v)
        }
        (Value::Map(mx), Value::Map(my)) => {
            let extra = ((mx.len() + my.len()) * 3) as u64;
            vm.charge_ops(extra)?;
            Value::Map(Value::map_merge_java(mx, my))
        }
        _ => Value::Number(a.to_real_for_compare() + b.to_real_for_compare()),
    };
    vm.push_stack(out)?;
    Ok(())
}

fn op_sub(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    let a = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    vm.push_stack(Value::Number(a - b))?;
    Ok(())
}

fn op_mul(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    let a = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    vm.push_stack(Value::Number(a * b))?;
    Ok(())
}

fn op_div(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    let a = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    if b == 0.0 {
        return Err(VmError::DivByZero);
    }
    vm.push_stack(Value::Number(a / b))?;
    Ok(())
}

fn op_mod(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    let a = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    if b == 0.0 {
        return Err(VmError::DivByZero);
    }
    vm.push_stack(Value::Number(a % b))?;
    Ok(())
}

fn op_neg(vm: &mut Vm) -> Result<(), VmError> {
    let x = vm.pop_stack()?.as_number().ok_or(VmError::ExpectedNumber)?;
    vm.push_stack(Value::Number(-x))?;
    Ok(())
}

fn store_local(vm: &mut Vm, i: u16, v: Value) -> Result<(), VmError> {
    let (old_q, new_q) = {
        let slot = vm
            .locals
            .get_mut(usize::from(i))
            .ok_or(VmError::BadLocal(i))?;
        let old = core::mem::replace(slot, v);
        (old.ram_quads(), slot.ram_quads())
    };
    vm.release_ram(old_q);
    vm.charge_ram(new_q)?;
    Ok(())
}

fn op_return(vm: &mut Vm) -> Result<(), VmError> {
    let ret = vm.pop_stack()?;
    if let Some(ret_pc) = vm.return_pcs.pop() {
        vm.pc = ret_pc;
        vm.push_stack(ret)?;
    } else {
        vm.push_stack(ret)?;
        vm.pc = vm.code.len();
    }
    Ok(())
}

fn op_get_local(vm: &mut Vm) -> Result<(), VmError> {
    let i = vm.read_u16()?;
    let v = vm
        .locals
        .get(usize::from(i))
        .cloned()
        .ok_or(VmError::BadLocal(i))?;
    vm.push_stack(v)?;
    Ok(())
}

fn op_set_local(vm: &mut Vm) -> Result<(), VmError> {
    let i = vm.read_u16()?;
    let v = vm.pop_stack()?;
    store_local(vm, i, v)
}

fn apply_pc_delta(vm: &mut Vm, delta: i32) -> Result<(), VmError> {
    let base = i64::try_from(vm.pc).map_err(|_| VmError::UnexpectedEof)?;
    let new_pc = base
        .checked_add(i64::from(delta))
        .ok_or(VmError::UnexpectedEof)?;
    let len = i64::try_from(vm.code.len()).map_err(|_| VmError::UnexpectedEof)?;
    if new_pc < 0 || new_pc > len {
        return Err(VmError::UnexpectedEof);
    }
    vm.pc = usize::try_from(new_pc).map_err(|_| VmError::UnexpectedEof)?;
    Ok(())
}

fn op_jump(vm: &mut Vm) -> Result<(), VmError> {
    let delta = vm.read_i32()?;
    apply_pc_delta(vm, delta)
}

fn op_jump_if_false(vm: &mut Vm) -> Result<(), VmError> {
    let delta = vm.read_i32()?;
    let cond = vm.pop_stack()?;
    if cond.truthy() {
        return Ok(());
    }
    apply_pc_delta(vm, delta)
}

fn op_call_native(vm: &mut Vm) -> Result<(), VmError> {
    let id = vm.read_u16()? as usize;
    let argc = vm.read_u8()? as usize;
    let native = vm
        .natives
        .get(id)
        .copied()
        .ok_or(VmError::BadNativeIndex(id as u16))?;
    let mut args = vec![Value::Null; argc];
    for slot in (0..argc).rev() {
        args[slot] = vm.pop_stack()?;
    }
    let out = native(vm, &args)?;
    vm.push_stack(out)?;
    Ok(())
}

fn op_eq_equals(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    vm.push_stack(Value::Bool(a.equals_equals_v4(&b)))?;
    Ok(())
}

fn op_ne_equals(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    vm.push_stack(Value::Bool(!a.equals_equals_v4(&b)))?;
    Ok(())
}

fn op_lt(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    vm.push_stack(Value::Bool(
        a.to_real_for_compare() < b.to_real_for_compare(),
    ))?;
    Ok(())
}

fn op_lte(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    vm.push_stack(Value::Bool(
        a.to_real_for_compare() <= b.to_real_for_compare(),
    ))?;
    Ok(())
}

fn op_gt(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    vm.push_stack(Value::Bool(
        a.to_real_for_compare() > b.to_real_for_compare(),
    ))?;
    Ok(())
}

fn op_gte(vm: &mut Vm) -> Result<(), VmError> {
    let b = vm.pop_stack()?;
    let a = vm.pop_stack()?;
    vm.push_stack(Value::Bool(
        a.to_real_for_compare() >= b.to_real_for_compare(),
    ))?;
    Ok(())
}

fn op_not(vm: &mut Vm) -> Result<(), VmError> {
    let v = vm.pop_stack()?;
    vm.push_stack(Value::Bool(!v.truthy()))?;
    Ok(())
}

fn op_array_build(vm: &mut Vm) -> Result<(), VmError> {
    let n = vm.read_u16()? as usize;
    let mut elems = Vec::with_capacity(n);
    for _ in 0..n {
        elems.push(vm.pop_stack()?);
    }
    elems.reverse();
    vm.push_stack(Value::Array(elems))?;
    Ok(())
}

fn op_map_build(vm: &mut Vm) -> Result<(), VmError> {
    let n = vm.read_u16()? as usize;
    let mut pairs = Vec::with_capacity(n);
    for _ in 0..n {
        let val = vm.pop_stack()?;
        let key = vm.pop_stack()?;
        pairs.push((key, val));
    }
    pairs.reverse();
    vm.push_stack(Value::Map(pairs))?;
    Ok(())
}

fn array_get(arr: &[Value], key: &Value) -> Value {
    let Some(n) = key.as_number() else {
        return Value::Null;
    };
    if !n.is_finite() {
        return Value::Null;
    }
    let i = n as i64;
    if i < 0 {
        return Value::Null;
    }
    let u = i as usize;
    arr.get(u).cloned().unwrap_or(Value::Null)
}

fn op_get_elem(vm: &mut Vm) -> Result<(), VmError> {
    let key = vm.pop_stack()?;
    let container = vm.pop_stack()?;
    let out = match &container {
        Value::Array(arr) => array_get(arr, &key),
        Value::Map(pairs) => pairs
            .iter()
            .find(|(k, _)| k == &key)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null),
        _ => Value::Null,
    };
    vm.push_stack(out)?;
    Ok(())
}

fn op_array_len(vm: &mut Vm) -> Result<(), VmError> {
    let v = vm.pop_stack()?;
    let n = match &v {
        Value::Array(a) => a.len() as f64,
        _ => 0.0,
    };
    vm.push_stack(Value::Number(n))?;
    Ok(())
}

fn op_map_len(vm: &mut Vm) -> Result<(), VmError> {
    let v = vm.pop_stack()?;
    let n = match &v {
        Value::Map(m) => m.len() as f64,
        _ => 0.0,
    };
    vm.push_stack(Value::Number(n))?;
    Ok(())
}

fn op_map_entry_at(vm: &mut Vm) -> Result<(), VmError> {
    let idx_v = vm.pop_stack()?;
    let map_v = vm.pop_stack()?;
    let (k, val) = match (&map_v, idx_v.as_number()) {
        (Value::Map(m), Some(n)) if n.is_finite() && n >= 0.0 => {
            let i = n as usize;
            m.get(i)
                .map(|p| (p.0.clone(), p.1.clone()))
                .unwrap_or((Value::Null, Value::Null))
        }
        _ => (Value::Null, Value::Null),
    };
    vm.push_stack(k)?;
    vm.push_stack(val)?;
    Ok(())
}

fn op_call_function(vm: &mut Vm) -> Result<(), VmError> {
    let fid = vm.read_u16()?;
    let argc = vm.read_u8()?;
    let meta = vm
        .functions
        .get(fid as usize)
        .cloned()
        .ok_or(VmError::BadFunctionIndex(fid))?;
    if argc != meta.argc {
        return Err(VmError::BadFunctionArity {
            expected: meta.argc,
            got: argc,
        });
    }
    let base = usize::from(meta.slot_base);
    let total = usize::from(meta.slot_count);
    if base.saturating_add(total) > vm.locals.len() {
        return Err(VmError::BadLocal(meta.slot_base.saturating_add(meta.slot_count).saturating_sub(1)));
    }
    let mut args = vec![Value::Null; argc as usize];
    for slot in (0..argc as usize).rev() {
        args[slot] = vm.pop_stack()?;
    }
    vm.return_pcs.push(vm.pc);
    for (i, v) in args.into_iter().enumerate() {
        let li = u16::try_from(base + i).map_err(|_| VmError::UnexpectedEof)?;
        store_local(vm, li, v)?;
    }
    for i in (argc as usize)..total {
        let li = u16::try_from(base + i).map_err(|_| VmError::UnexpectedEof)?;
        store_local(vm, li, Value::Null)?;
    }
    vm.pc = meta.entry_pc;
    Ok(())
}

fn op_try_begin(vm: &mut Vm) -> Result<(), VmError> {
    let catch_pc = vm.read_u32()? as usize;
    if catch_pc > vm.code.len() {
        return Err(VmError::UnexpectedEof);
    }
    vm.try_catch_pcs.push(catch_pc);
    Ok(())
}

fn op_try_end(vm: &mut Vm) -> Result<(), VmError> {
    vm.try_catch_pcs.pop().ok_or(VmError::TryStackUnderflow)?;
    Ok(())
}

fn op_throw(vm: &mut Vm) -> Result<(), VmError> {
    let v = vm.pop_stack()?;
    let Some(catch_pc) = vm.try_catch_pcs.pop() else {
        return Err(VmError::UncaughtThrow(v));
    };
    vm.pc = catch_pc;
    vm.push_stack(v)?;
    Ok(())
}
