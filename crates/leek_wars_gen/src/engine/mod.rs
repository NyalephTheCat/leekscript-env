mod java;
mod rust;

pub use java::{
    default_java_cwd, dump_java_fight_bootstrap, JavaEngine, JavaEngineConfig, JavaFightBootstrap,
};
pub use rust::RustEngine;

use std::path::{Path, PathBuf};

/// Arguments shared by engines (mirrors flags understood by `com.leekwars.Main`).
#[derive(Debug, Clone, Default)]
pub struct RunRequest {
    pub nocache: bool,
    pub dbresolver: bool,
    pub verbose: bool,
    pub analyze: bool,
    pub farmer: Option<i32>,
    pub folder: Option<i32>,
    /// Scenario or `.leek` path for `--analyze`.
    pub file: PathBuf,
}

impl RunRequest {
    /// Forward argv fragments for the Java `main` (excluding `java -jar …`).
    #[must_use]
    pub fn java_argv(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.nocache {
            v.push("--nocache".to_string());
        }
        if self.dbresolver {
            v.push("--dbresolver".to_string());
        }
        if self.verbose {
            v.push("--verbose".to_string());
        }
        if self.analyze {
            v.push("--analyze".to_string());
        }
        if let Some(f) = self.farmer {
            v.push(format!("--farmer={f}"));
        }
        if let Some(f) = self.folder {
            v.push(format!("--folder={f}"));
        }
        v.push(self.file.display().to_string());
        v
    }
}

/// Resolve `generator.jar`: `LEEK_GENERATOR_JAR`, then `leek-wars-generator/generator.jar` relative to the workspace root.
pub fn resolve_generator_jar() -> Result<PathBuf, crate::GenError> {
    if let Ok(p) = std::env::var("LEEK_GENERATOR_JAR") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Ok(pb);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(jar) = search_jar_upward(&cwd) {
            return Ok(jar);
        }
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(jar) = search_jar_upward(&base) {
        return Ok(jar);
    }
    Err(crate::GenError::JarNotFound)
}

fn search_jar_upward(mut dir: &Path) -> Option<PathBuf> {
    loop {
        let candidate = dir.join("leek-wars-generator/generator.jar");
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
        let candidate = dir.join("generator.jar");
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
        dir = dir.parent()?;
    }
}
