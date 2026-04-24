use crate::error::GenError;
use leekscript_config::find_manifest;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct GeneratorConfig {
    pub manifest_path: Option<PathBuf>,
    pub root: PathBuf,
    pub scenarios_dir: PathBuf,
    pub ai_dir: PathBuf,
    pub output: OutputFormat,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratorConfigExplain {
    pub root: FieldSource,
    pub scenarios_dir: FieldSource,
    pub ai_dir: FieldSource,
    pub output: FieldSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    Cli,
    Env,
    Manifest,
    Default,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Pretty,
    Json,
    Ndjson,
}

impl OutputFormat {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.trim() {
            "pretty" => Some(Self::Pretty),
            "json" => Some(Self::Json),
            "ndjson" => Some(Self::Ndjson),
            _ => None,
        }
    }
}

fn cwd_for_search() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn env_root() -> Option<PathBuf> {
    std::env::var("LEEK_GENERATOR_CWD")
        .ok()
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn default_root() -> PathBuf {
    // Best-effort: use cwd, which is typically the workspace root when invoked via `cargo run`.
    cwd_for_search()
}

fn resolve_manifest_path() -> Option<PathBuf> {
    find_manifest(cwd_for_search())
}

fn read_manifest_toml(path: &Path) -> Result<toml::Value, GenError> {
    let s = std::fs::read_to_string(path)?;
    let v: toml::Value = toml::from_str(&s).map_err(|e| GenError::Message(e.to_string()))?;
    Ok(v)
}

fn table_get_string(tbl: &toml::value::Table, key: &str) -> Option<String> {
    tbl.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn table_get_path(tbl: &toml::value::Table, key: &str, base: &Path) -> Option<PathBuf> {
    let s = table_get_string(tbl, key)?;
    let p = PathBuf::from(s);
    Some(if p.is_absolute() { p } else { base.join(p) })
}

/// Resolve generator configuration.
///
/// Precedence:
/// - explicit CLI root overrides everything
/// - `LEEK_GENERATOR_CWD`
/// - `[generator]` in `Leek.toml` (walk-up discovery)
/// - built-in defaults
pub fn resolve(cli_root: Option<PathBuf>) -> Result<GeneratorConfig, GenError> {
    resolve_with_explain(cli_root).map(|(cfg, _)| cfg)
}

pub fn resolve_with_explain(
    cli_root: Option<PathBuf>,
) -> Result<(GeneratorConfig, GeneratorConfigExplain), GenError> {
    let manifest_path = resolve_manifest_path();
    let mut root_from_manifest: Option<PathBuf> = None;
    let mut scenarios_dir: Option<PathBuf> = None;
    let mut ai_dir: Option<PathBuf> = None;
    let mut output: Option<OutputFormat> = None;

    if let Some(ref mp) = manifest_path {
        let base = mp.parent().unwrap_or_else(|| Path::new("."));
        let v = read_manifest_toml(mp.as_path()).map_err(|e| {
            GenError::Message(format!("failed to parse {}: {e}", mp.display()))
        })?;
        if let Some(gen) = v.get("generator").and_then(|t| t.as_table()) {
            root_from_manifest = table_get_path(gen, "generator_root", base);
            scenarios_dir = table_get_path(gen, "scenarios_dir", base);
            ai_dir = table_get_path(gen, "ai_dir", base);
            output = table_get_string(gen, "output").and_then(|s| OutputFormat::parse_str(&s));
        }
    }

    let (root, root_src) = if let Some(r) = cli_root {
        (r, FieldSource::Cli)
    } else if let Some(r) = env_root() {
        (r, FieldSource::Env)
    } else if let Some(r) = root_from_manifest.clone() {
        (r, FieldSource::Manifest)
    } else {
        (default_root(), FieldSource::Default)
    };

    let (scenarios_dir, scenarios_src) = if let Some(p) = scenarios_dir {
        (p, FieldSource::Manifest)
    } else {
        (root.join("test/scenario"), FieldSource::Default)
    };
    let (ai_dir, ai_src) = if let Some(p) = ai_dir {
        (p, FieldSource::Manifest)
    } else {
        (root.join("test/ai"), FieldSource::Default)
    };
    let (output, output_src) = if let Some(o) = output {
        (o, FieldSource::Manifest)
    } else {
        (OutputFormat::default(), FieldSource::Default)
    };

    Ok((
        GeneratorConfig {
        manifest_path,
        root,
        scenarios_dir,
        ai_dir,
        output,
        },
        GeneratorConfigExplain {
            root: root_src,
            scenarios_dir: scenarios_src,
            ai_dir: ai_src,
            output: output_src,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "leekgen_config_test_{}_{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn cli_root_overrides_env_root() {
        let _g = ENV_LOCK.lock().unwrap();
        let env_root = tmp_dir("env");
        let cli_root = tmp_dir("cli");

        std::env::set_var("LEEK_GENERATOR_CWD", env_root.to_string_lossy().to_string());
        let cfg = resolve(Some(cli_root.clone())).expect("resolve");
        assert_eq!(cfg.root, cli_root);

        std::env::remove_var("LEEK_GENERATOR_CWD");
        let _ = std::fs::remove_dir_all(env_root);
        let _ = std::fs::remove_dir_all(cli_root);
    }

    #[test]
    fn env_root_used_when_no_cli_root() {
        let _g = ENV_LOCK.lock().unwrap();
        let env_root = tmp_dir("env_only");

        std::env::set_var("LEEK_GENERATOR_CWD", env_root.to_string_lossy().to_string());
        let cfg = resolve(None).expect("resolve");
        assert_eq!(cfg.root, env_root);

        std::env::remove_var("LEEK_GENERATOR_CWD");
        let _ = std::fs::remove_dir_all(cfg.root);
    }
}

