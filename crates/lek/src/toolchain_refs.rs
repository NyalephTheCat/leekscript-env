//! References emitted by the Rust toolchain (HIR lowering + interpreter). Must stay aligned with
//! [`leekscript_hir::refs::EMITTED_REFERENCES`] and [`leekscript_run::INTERP_EMITTED_REFERENCES`].

use leekscript_diagnostics::Registry;

/// All Java-style `reference` ids the current `lek check` / `lek run` can emit (compile + run).
#[must_use]
pub fn all_emitted_references() -> Vec<&'static str> {
    leekscript_hir::refs::EMITTED_REFERENCES
        .iter()
        .chain(leekscript_resolve::EMITTED_REFERENCES.iter())
        .chain(leekscript_run::INTERP_EMITTED_REFERENCES.iter())
        .copied()
        .collect()
}

/// Returns an error string if any emitted reference is missing from `registry`.
pub fn verify_emitted_references(registry: &Registry) -> Result<(), String> {
    let missing = registry.missing_references(&all_emitted_references());
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "registry is missing {} reference(s) used by the toolchain: {}",
            missing.len(),
            missing.join(", ")
        ))
    }
}
