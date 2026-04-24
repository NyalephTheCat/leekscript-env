//! `lek fmt` — token-based formatting (see `leekscript_fmt`).

use leekscript_directives::parse_file_preamble;
use leekscript_fmt::{fmt_config_from_workspace, format_source, FmtConfig, FormatError};
use leekscript_lexer::LexerConfig;

use crate::check::manifest_language_settings;

#[derive(Clone, Debug, Default)]
pub struct FmtOptions {
    pub manifest: Option<std::path::PathBuf>,
    pub cli_language_version: Option<u8>,
    pub cli_width: Option<u32>,
    pub cli_indent: Option<u32>,
}

/// Format UTF-8 source using `[fmt]` + `// leek-fmt:` + CLI overrides (same language resolution as `check`).
pub fn format_one_file(src: &str, opts: &FmtOptions) -> Result<String, FormatError> {
    let (manifest_version, _) = manifest_language_settings(opts.manifest.as_ref());
    let (preamble, _) = parse_file_preamble(src, crate::check::PREAMBLE_MAX_LINES);
    let file_lang = opts
        .cli_language_version
        .or(preamble.language_version)
        .or(manifest_version)
        .unwrap_or(4);

    let mut cfg: FmtConfig = fmt_config_from_workspace(opts.manifest.as_ref());
    cfg = cfg.merge_preamble(preamble.fmt.as_ref());
    if let Some(w) = opts.cli_width {
        cfg.width = w;
    }
    if let Some(i) = opts.cli_indent {
        cfg.indent = i;
    }

    format_source(src, &cfg, LexerConfig { version: file_lang })
}
