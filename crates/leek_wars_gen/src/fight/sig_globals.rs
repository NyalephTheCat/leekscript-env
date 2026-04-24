//! Fight compile + interpreter globals from workspace `.sig.leek` files.
//!
//! Source of truth (edit in-repo): `data/signatures/core.sig.leek` and `data/signatures/leekwars.sig.leek`.
//! Loaded via [`leekscript_run::merged_workspace_sig_bundle`] (HIR parse, not regex).

use leekscript_run::InterpretSession;
use leekscript_run::SigLeekUnit;
use std::collections::HashSet;
use std::sync::OnceLock;

static BUNDLE: OnceLock<SigLeekUnit> = OnceLock::new();

fn bundle() -> &'static SigLeekUnit {
    BUNDLE.get_or_init(leekscript_run::merged_workspace_sig_bundle)
}

/// Append names from `core.sig.leek` + `leekwars.sig.leek` for [`leekscript_run::CompileOptions::signature_globals`].
pub fn merge_signature_globals(mut from_toml: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = from_toml.iter().cloned().collect();
    for name in &bundle().names {
        if seen.insert(name.clone()) {
            from_toml.push(name.clone());
        }
    }
    from_toml
}

/// Integer globals with literal initializers from the merged signature bundle (weapons, chips, stdlib ints, …).
pub fn seed_interpret_session(sess: &mut InterpretSession) {
    sess.seed_global_integers(&bundle().integer_globals);
}

#[cfg(test)]
mod tests {
    use super::merge_signature_globals;
    use leekscript_signatures::SignatureFile;
    use std::path::Path;

    #[test]
    fn merged_signature_globals_include_leekwars_effect_constants() {
        let sig = SignatureFile::load_path(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/wars_functions.toml"),
        )
        .expect("wars_functions.toml");
        let merged = merge_signature_globals(sig.resolve_names());
        assert!(
            merged.iter().any(|n| n == "EFFECT_POISON"),
            "expected EFFECT_POISON in merged globals (len {})",
            merged.len()
        );
    }
}
