//! Syntax-tree-aware mutation of LeekScript sources for fuzzing and differential testing.
//!
//! ## Acceptance policies
//!
//! - [`MutantAcceptance::AcceptAll`] — always return a candidate (same behavior as historical `leekgen` fuzz):
//!   CST edits when the file parses, otherwise a trailing marker comment and optional digit noise.
//! - [`MutantAcceptance::RequireParseable`] — retry until the output parses (any tried lexer version), or
//!   return the original source with [`OutcomeKind::RejectedAllAttempts`].
//! - [`MutantAcceptance::RequireCompilable`] — retry until [`leekscript_run::compile_source`] succeeds.
//!   Requires [`MutateSettings::compile`]. Use a [`leekscript_run::CompileOptions`] that matches how you
//!   compile the real file (signatures, `source_path` for `include`, etc.).

mod mutate;

pub use mutate::{
    generate_mutant_candidate, generate_mutant_candidate_with_settings, mutate_leek_source, parse_best,
    source_parses_any_version,
};

use leekscript_run::CompileOptions;
use thiserror::Error;

/// Configuration for statement / expression injection during CST mutation.
///
/// These settings only matter for higher mutation levels (currently `level >= 4`).
#[derive(Clone, Debug)]
pub struct InjectSettings {
    /// `0` = off (no injected blocks), higher values allow more complex injected code.
    pub complexity: u8,
    /// Percent chance (0..=100) to offer a “wrap this statement in a block + injected code” mutation per statement node.
    pub wrap_percent: u8,
    /// Maximum number of injected statements to append when a wrap mutation is chosen.
    pub max_injected_stmts: u8,
    /// Percent chance (0..=100) to generate *scope-aware* statements that reuse existing identifiers.
    /// (e.g. assign to a variable declared in the file, call a function-valued variable, etc.)
    pub scope_aware_percent: u8,
}

impl Default for InjectSettings {
    fn default() -> Self {
        Self {
            complexity: 2,
            wrap_percent: 55,
            max_injected_stmts: 3,
            scope_aware_percent: 65,
        }
    }
}

/// What to do when a generated mutant fails the chosen gate (parse or compile).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MutantAcceptance {
    /// Do not filter: return the first candidate (may be syntactically invalid if fallback ran).
    #[default]
    AcceptAll,
    /// Keep trying until [`source_parses_any_version`] passes, up to [`MutateSettings::max_attempts`].
    RequireParseable,
    /// Keep trying until `compile_source` succeeds, up to [`MutateSettings::max_attempts`].
    RequireCompilable,
}

/// Context for [`MutantAcceptance::RequireCompilable`].
#[derive(Clone, Debug)]
pub struct CompileCheckContext {
    /// First argument to [`leekscript_run::compile_source`].
    pub path_display: String,
    pub options: CompileOptions,
}

/// Configuration for [`mutate_leek_source`].
#[derive(Clone, Debug)]
pub struct MutateSettings {
    pub acceptance: MutantAcceptance,
    /// Random retries when a candidate is rejected (parse/compile gate).
    pub max_attempts: u32,
    /// Required when [`MutantAcceptance::RequireCompilable`] is set.
    pub compile: Option<CompileCheckContext>,
    /// Statement / expression injection controls (used by higher mutation levels).
    pub inject: InjectSettings,
    /// When true, skip edits that often diverge the official Java AI pipeline from Rust (`fight.actions`),
    /// such as redundant parens on literals and `==`/`!=` flips. Use with `--fuzz-parity`.
    pub parity_safe: bool,
}

impl Default for MutateSettings {
    fn default() -> Self {
        Self {
            acceptance: MutantAcceptance::AcceptAll,
            max_attempts: 64,
            compile: None,
            inject: InjectSettings::default(),
            parity_safe: false,
        }
    }
}

impl MutateSettings {
    pub fn accept_all() -> Self {
        Self::default()
    }

    pub fn require_parseable() -> Self {
        Self {
            acceptance: MutantAcceptance::RequireParseable,
            max_attempts: 64,
            compile: None,
            inject: InjectSettings::default(),
            parity_safe: false,
        }
    }

    pub fn require_compilable(path_display: impl Into<String>, options: CompileOptions) -> Self {
        Self {
            acceptance: MutantAcceptance::RequireCompilable,
            max_attempts: 64,
            compile: Some(CompileCheckContext {
                path_display: path_display.into(),
                options,
            }),
            inject: InjectSettings::default(),
            parity_safe: false,
        }
    }
}

/// Result of [`mutate_leek_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutateOutcome {
    pub source: String,
    pub kind: OutcomeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeKind {
    /// `level == 0` or no change requested.
    NoOp,
    /// A mutant was accepted (including [`MutantAcceptance::AcceptAll`] on first try).
    Mutated,
    /// Every attempt failed the gate; `source` is the original input.
    RejectedAllAttempts { attempts: u32 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MutateError {
    #[error("MutantAcceptance::RequireCompilable requires MutateSettings::compile")]
    MissingCompileContext,
}
