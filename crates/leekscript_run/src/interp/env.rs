//! Lexical environment (scope stack).

use super::error::InterpretError;
use super::value::{SharedArray, Value};
use std::collections::HashMap;

pub(super) struct Env {
    scopes: Vec<HashMap<String, Value>>,
    /// Per scope: names that alias `array[index]` (Leek `@` parameters / `for (var @x in arr)`).
    array_refs: Vec<HashMap<String, (SharedArray, usize)>>,
    /// Leek v1: `function(@p)` with call `f(b)` — `p` reads/writes the caller binding `b`.
    var_aliases: Vec<HashMap<String, String>>,
    /// Scope index floor for unqualified reads inside user functions (skip script `var` shadowing instance fields).
    callable_scope_floor_stack: Vec<usize>,
}

impl Env {
    pub(super) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            array_refs: vec![HashMap::new()],
            var_aliases: vec![HashMap::new()],
            callable_scope_floor_stack: Vec::new(),
        }
    }

    pub(super) fn push_block(&mut self) {
        self.scopes.push(HashMap::new());
        self.array_refs.push(HashMap::new());
        self.var_aliases.push(HashMap::new());
    }

    /// Pop the innermost block scope and return bindings dropped from that scope (for Java-style RAM).
    pub(super) fn pop_block(&mut self) -> Vec<Value> {
        if self.scopes.len() > 1 {
            self.array_refs.pop();
            self.var_aliases.pop();
            if let Some(m) = self.scopes.pop() {
                return m.into_values().collect();
            }
        }
        Vec::new()
    }

    /// Pop a scope without returning bindings (e.g. callable parameter frame: no separate RAM charge on entry).
    pub(super) fn pop_block_silent(&mut self) {
        if self.scopes.len() > 1 {
            self.array_refs.pop();
            self.var_aliases.pop();
            self.scopes.pop();
        }
    }

    pub(super) fn insert(&mut self, name: String, val: Value) {
        self.insert_maybe_array_cell(name, val, None);
    }

    pub(super) fn insert_maybe_array_cell(
        &mut self,
        name: String,
        val: Value,
        array_cell: Option<(SharedArray, usize)>,
    ) {
        let li = self.scopes.len().saturating_sub(1);
        if let Some(m) = self.scopes.get_mut(li) {
            m.insert(name.clone(), val);
        }
        if let (Some(m), Some(ac)) = (self.array_refs.get_mut(li), array_cell) {
            m.insert(name, ac);
        }
    }

    pub(super) fn insert_var_alias(&mut self, param: String, caller_var: String) {
        let li = self.scopes.len().saturating_sub(1);
        if let Some(m) = self.var_aliases.get_mut(li) {
            m.insert(param, caller_var);
        }
    }

    /// Like top-level / `global` in Java: bind in the outermost scope regardless of block depth.
    pub(super) fn insert_global(&mut self, name: String, val: Value) {
        if let Some(m) = self.scopes.first_mut() {
            m.insert(name, val);
        }
    }

    pub(super) fn contains_global(&self, name: &str) -> bool {
        self.scopes.first().is_some_and(|m| m.contains_key(name))
    }

    pub(super) fn get(&self, name: &str) -> Option<Value> {
        let n = self.scopes.len();
        for si in (0..n).rev() {
            if let Some((arr, idx)) = self.array_refs[si].get(name) {
                return arr.borrow().get(*idx).cloned();
            }
            if let Some(target) = self.var_aliases[si].get(name) {
                return self.get(target);
            }
            if let Some(v) = self.scopes[si].get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn lookup_in_scope_range(&self, name: &str, range: std::ops::Range<usize>) -> Option<Value> {
        for si in range.rev() {
            if let Some((arr, idx)) = self.array_refs[si].get(name) {
                return arr.borrow().get(*idx).cloned();
            }
            if let Some(target) = self.var_aliases[si].get(name) {
                return self.get(target);
            }
            if let Some(v) = self.scopes[si].get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    /// Locals / parameters in the current user function (inner blocks included), not outer script `var`.
    pub(super) fn get_callable_local(&self, name: &str) -> Option<Value> {
        let floor = self.callable_scope_floor_stack.last().copied().unwrap_or(0);
        self.lookup_in_scope_range(name, floor..self.scopes.len())
    }

    /// Scopes outside the current callable frame (globals, enclosing blocks, closures).
    pub(super) fn get_callable_outer_lexical(&self, name: &str) -> Option<Value> {
        let floor = self.callable_scope_floor_stack.last().copied().unwrap_or(0);
        self.lookup_in_scope_range(name, 0..floor)
    }

    pub(super) fn begin_callable_frame(&mut self) {
        self.push_block();
        let floor = self.scopes.len().saturating_sub(1);
        self.callable_scope_floor_stack.push(floor);
    }

    pub(super) fn end_callable_frame(&mut self) {
        self.callable_scope_floor_stack.pop();
        self.pop_block_silent();
    }

    pub(super) fn in_user_callable(&self) -> bool {
        !self.callable_scope_floor_stack.is_empty()
    }

    pub(super) fn callable_outer_lexical_is_array_ref(&self, name: &str) -> bool {
        let floor = self.callable_scope_floor_stack.last().copied().unwrap_or(0);
        for si in (0..floor).rev() {
            if self.array_refs[si].contains_key(name) {
                return true;
            }
        }
        false
    }

    pub(super) fn is_aliased(&self, name: &str) -> bool {
        self.var_aliases.iter().any(|m| m.contains_key(name))
    }

    pub(super) fn snapshot_callable_visible_non_global(&self) -> HashMap<String, Value> {
        let mut out = HashMap::new();
        // Closures should not capture the global/script frame by value: reads are allowed via
        // outer lexical lookup, but mutating through captured aliases must still hit the true global.
        for si in 1..self.scopes.len() {
            for (k, v) in &self.scopes[si] {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    pub(super) fn snapshot_callable_aliases_non_global(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for si in 1..self.var_aliases.len() {
            for (k, v) in &self.var_aliases[si] {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    /// Updates the innermost scope that already holds `name`.
    ///
    /// Returns the previous binding value when the name referred to a real stack slot (not a bare
    /// alias redirect), so the interpreter can subtract Java-style RAM for dropped containers.
    pub(super) fn assign(
        &mut self,
        name: &str,
        val: Value,
    ) -> Result<Option<Value>, InterpretError> {
        let n = self.scopes.len();
        for si in (0..n).rev() {
            if let Some(target) = self.var_aliases[si].get(name) {
                let t = target.clone();
                return self.assign(&t, val);
            }
            if let Some((arr, idx)) = self.array_refs[si].get(name) {
                let arr = arr.clone();
                let idx = *idx;
                let old = {
                    let mut b = arr.borrow_mut();
                    std::mem::replace(&mut b[idx], val.clone())
                };
                if let Some(m) = self.scopes[si].get_mut(name) {
                    *m = val;
                }
                return Ok(Some(old));
            }
            if self.scopes[si].contains_key(name) {
                return Ok(self.scopes[si].insert(name.to_string(), val));
            }
        }
        Err(InterpretError::variable_not_exists(name))
    }
}
