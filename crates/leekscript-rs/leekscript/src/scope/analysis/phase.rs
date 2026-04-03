/// Which pass of the two-phase analysis is running.
///
/// See [`crate::scope::analysis`] module docs for invariants between phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalysisPhase {
    /// First tree walk: allocate scopes, declare symbols, record [`ScopeGraph::binding_spans`].
    BuildScopes,
    /// Second walk: replay scope pushes, resolve references, infer expression types, narrowing.
    ResolveAndInfer,
}

impl AnalysisPhase {
    #[must_use]
    pub(crate) fn is_build_scopes(self) -> bool {
        matches!(self, Self::BuildScopes)
    }
}
