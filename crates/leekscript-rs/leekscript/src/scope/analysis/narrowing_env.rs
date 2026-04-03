use std::collections::HashMap;

use crate::scope::leek_ty::LeekTy;
use crate::scope::model::SymbolId;

/// Active control-flow narrowing: stack of per-region symbol → refined type maps.
#[derive(Default)]
pub(crate) struct NarrowingEnv {
    stack: Vec<HashMap<SymbolId, LeekTy>>,
}

impl NarrowingEnv {
    pub(crate) fn push_frame(&mut self, facts: HashMap<SymbolId, LeekTy>) {
        self.stack.push(facts);
    }

    pub(crate) fn pop_frame(&mut self) {
        let _ = self.stack.pop();
    }

    /// Apply innermost narrowing for `sid` on top of `base` (e.g. declared/inferred type).
    #[must_use]
    pub(crate) fn with_narrowing(&self, sid: SymbolId, base: LeekTy) -> LeekTy {
        let mut t = base;
        if let Some(top) = self.stack.last() {
            if let Some(nt) = top.get(&sid) {
                t = nt.clone();
            }
        }
        t
    }
}
