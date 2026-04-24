//! Java `AI.mRAM` / `RamUsage` parity helpers (quad counts; approximate for the tree interpreter).

use super::context::InterpCx;
use super::error::InterpretError;
use super::value::Value;
use std::rc::Rc;

/// Java `MapLeekValue` / `ObjectLeekValue` field storage: two quads per entry (key+value pair).
pub(super) const MAP_RAM_QUADS_PER_ENTRY: u64 = 2;

/// Charge top-level container storage like Java's `allocateRAM` / `increaseRAM` on the root object.
pub(super) fn charge_top_level_container_ram(
    cx: &mut InterpCx,
    v: &Value,
) -> Result<(), InterpretError> {
    match v {
        Value::Array(a) => cx.charge_ram_quads(a.borrow().len() as u64),
        Value::Map(m) | Value::Object(m) => {
            cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * m.borrow().len() as u64)
        }
        Value::Set(s) => cx.charge_ram_quads(s.borrow().elems.len() as u64),
        Value::Instance(i) => {
            let b = i.borrow();
            let mut n = MAP_RAM_QUADS_PER_ENTRY * b.fields.len() as u64;
            if let Some(a) = &b.array_backing {
                n = n.saturating_add(a.borrow().len() as u64);
            }
            cx.charge_ram_quads(n)
        }
        _ => Ok(()),
    }
}

/// When a sole-owned binding value is dropped (reassign or scope pop), mirror Java GC freeing the
/// object's `RamUsage` tracker (`RamUsage::free`).
pub(super) fn release_owned_binding_value_ram(cx: &mut InterpCx, v: Value) {
    match v {
        Value::Array(a) => {
            if let Ok(cell) = Rc::try_unwrap(a) {
                let vec = cell.into_inner();
                cx.release_ram_quads(vec.len() as u64);
                for x in vec {
                    release_owned_binding_value_ram(cx, x);
                }
            }
        }
        Value::Map(m) | Value::Object(m) => {
            if let Ok(cell) = Rc::try_unwrap(m) {
                let store = cell.into_inner();
                let n = MAP_RAM_QUADS_PER_ENTRY * store.len() as u64;
                cx.release_ram_quads(n);
                for (k, vv) in store {
                    release_owned_binding_value_ram(cx, k);
                    release_owned_binding_value_ram(cx, vv);
                }
            }
        }
        Value::Set(s) => {
            if let Ok(cell) = Rc::try_unwrap(s) {
                let data = cell.into_inner();
                cx.release_ram_quads(data.elems.len() as u64);
                for x in data.elems {
                    release_owned_binding_value_ram(cx, x);
                }
            }
        }
        Value::Instance(rc) => {
            if let Ok(cell) = Rc::try_unwrap(rc) {
                let mut data = cell.into_inner();
                let n = MAP_RAM_QUADS_PER_ENTRY * data.fields.len() as u64;
                cx.release_ram_quads(n);
                let fields = std::mem::take(&mut data.fields);
                let ab = data.array_backing.take();
                drop(data);
                for (_, fv) in fields {
                    release_owned_binding_value_ram(cx, fv);
                }
                if let Some(a) = ab {
                    release_owned_binding_value_ram(cx, Value::Array(a));
                }
            }
        }
        _ => {}
    }
}

pub(super) fn release_dropped_binding_values_ram(cx: &mut InterpCx, vals: Vec<Value>) {
    for v in vals {
        release_owned_binding_value_ram(cx, v);
    }
}

/// After mutating a map/object slot count, keep `ram_quads_used` at least the live map footprint.
pub(super) fn note_keyed_container_ram_peak(
    cx: &mut InterpCx,
    entry_count: usize,
) -> Result<(), InterpretError> {
    let q = MAP_RAM_QUADS_PER_ENTRY * entry_count as u64;
    cx.ram_quads_used = cx.ram_quads_used.max(q);
    if cx.ram_quads_limit.is_some_and(|limit| q > limit) {
        return Err(InterpretError::out_of_memory());
    }
    Ok(())
}
