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

    /// Apply active narrowing for `sid` on top of `base` (e.g. declared/inferred type).
    ///
    /// Frames are ordered outer → inner; each map can refine symbols for its region. Later frames
    /// override earlier ones for the same symbol so an empty inner frame does not hide refinements
    /// from an outer frame (e.g. `||` RHS facts below a per-node narrowing push).
    #[must_use]
    pub(crate) fn with_narrowing(&self, sid: SymbolId, base: LeekTy) -> LeekTy {
        let mut t = base;
        for frame in &self.stack {
            if let Some(nt) = frame.get(&sid) {
                t = nt.clone();
            }
        }
        t
    }
}
