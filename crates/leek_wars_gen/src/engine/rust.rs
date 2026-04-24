use crate::engine::{default_java_cwd, resolve_generator_jar, RunRequest};
use crate::error::GenError;
use crate::fight::{run_scenario_path, run_scenario_path_with_ai_overlay};
use leekscript_run::{compile_source, CompileOptions};
use std::path::Path;

/// Placeholder for the in-tree fight simulator (state, chips, weapons, turns) backed by [`leekscript_run`].
#[derive(Debug, Clone, Default)]
pub struct RustEngine;

impl RustEngine {
    /// Run a scenario with an explicit AI / scenario base directory (typically `leek-wars-generator/`).
    pub fn run_scenario_with_cwd(
        &self,
        req: &RunRequest,
        ai_base: &Path,
    ) -> Result<String, GenError> {
        run_scenario_path(&req.file, ai_base)
    }

    /// Same as [`Self::run_scenario_with_cwd`], but prefer mutated `.leek` sources under `ai_overlay` when present.
    pub fn run_scenario_with_cwd_overlay(
        &self,
        req: &RunRequest,
        ai_base: &Path,
        ai_overlay: Option<&Path>,
    ) -> Result<String, GenError> {
        run_scenario_path_with_ai_overlay(&req.file, ai_base, ai_overlay)
    }

    /// Run a scenario through the in-tree fight loop (simplified physics vs Java).
    ///
    /// AI paths in the scenario are resolved relative to `LEEK_GENERATOR_CWD`, or — if unset —
    /// the directory containing `generator.jar` when that jar can be found.
    pub fn run_scenario(&self, req: &RunRequest) -> Result<String, GenError> {
        let cwd = std::env::var("LEEK_GENERATOR_CWD")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| {
                resolve_generator_jar()
                    .ok()
                    .map(|j| default_java_cwd(&j))
            })
            .ok_or_else(|| {
                GenError::Message(
                    "set LEEK_GENERATOR_CWD to your leek-wars-generator checkout (AI paths are relative to it)"
                        .into(),
                )
            })?;
        self.run_scenario_with_cwd(req, &cwd)
    }

    /// Compile a `.leek` file through the same pipeline as `lek run` (parse → HIR → resolve/types).
    /// Useful for benchmarking compiler work against Java without a full fight loop.
    pub fn compile_ai_file(&self, path: &Path) -> Result<(), GenError> {
        let src = std::fs::read_to_string(path)?;
        let opts = CompileOptions {
            source_path: Some(path.to_path_buf()),
            snippet_origin: Some(path.to_path_buf()),
            ..Default::default()
        };
        compile_source(path.display().to_string(), &src, &opts).map_err(|diags| {
            let msg = diags
                .iter()
                .map(|d| format!("{}: {}", d.reference, d.message))
                .collect::<Vec<_>>()
                .join("\n");
            GenError::Message(msg)
        })?;
        Ok(())
    }
}
