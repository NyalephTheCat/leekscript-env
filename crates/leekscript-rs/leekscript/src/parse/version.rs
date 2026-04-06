use sipha::prelude::*;

pub const FLAG_V1: FlagId = 1;
pub const FLAG_V2: FlagId = 2;
pub const FLAG_V3: FlagId = 3;
pub const FLAG_V4: FlagId = 4;
/// When set, module scope uses `recover_until` around each top-level statement (multi-error / IDE-style parse).
pub const FLAG_PARSE_RECOVERY: FlagId = 6;
/// When set, top-level `function` declarations may end with `;` instead of a `{ … }` block (API / stdlib stubs).
pub const FLAG_SIGNATURE_MODE: FlagId = 7;

/// `let` keyword and `let`/`var`-style declarations using it.
pub const FLAG_EXP_LET: FlagId = 8;
/// `const` declarations.
pub const FLAG_EXP_LEXICAL_CONST: FlagId = 9;
/// `match` statement.
pub const FLAG_EXP_MATCH: FlagId = 10;
/// `import` / `export` / `package`.
pub const FLAG_EXP_MODULES: FlagId = 11;
/// `try` / `catch` / `finally` / `throw`.
pub const FLAG_EXP_EXCEPTIONS: FlagId = 12;
/// `goto` statement.
pub const FLAG_EXP_GOTO: FlagId = 13;
/// `break N` / `continue N` numeric loop levels.
pub const FLAG_EXP_LOOP_LEVELS: FlagId = 14;
/// Default values on top-level / anonymous `function (…)`: `function f(a = 1) {}` (methods always allow `=`).
pub const FLAG_EXP_FN_OPTIONAL_PARAMS: FlagId = 15;
/// Template parameters on classes, top-level functions, and anonymous functions: `function id<T>(…)`, `class C<T>`, `function<T>(…) {}` (not arrow lambdas — `<T>` would clash with `<…>` set literals).
pub const FLAG_EXP_TEMPLATES: FlagId = 16;
/// While set, bare `>` is not parsed as relational greater-than (it closes `<…>` set literals).
pub const FLAG_IN_SET_LITERAL: FlagId = 17;

/// Optional parse features layered on top of a base [`Version`] (typically [`Version::V4`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExperimentalFeatures {
    pub let_bindings: bool,
    pub lexical_const: bool,
    pub match_stmt: bool,
    pub modules: bool,
    pub exceptions: bool,
    pub goto: bool,
    pub loop_levels: bool,
    pub fn_optional_params: bool,
    pub templates: bool,
}

impl Default for ExperimentalFeatures {
    fn default() -> Self {
        Self::NONE
    }
}

impl ExperimentalFeatures {
    pub const NONE: Self = Self {
        let_bindings: false,
        lexical_const: false,
        match_stmt: false,
        modules: false,
        exceptions: false,
        goto: false,
        loop_levels: false,
        fn_optional_params: false,
        templates: false,
    };

    pub const ALL: Self = Self {
        let_bindings: true,
        lexical_const: true,
        match_stmt: true,
        modules: true,
        exceptions: true,
        goto: true,
        loop_levels: true,
        fn_optional_params: true,
        templates: true,
    };

    #[must_use]
    pub fn merge_into_parse_context(self, mut ctx: ParseContext) -> ParseContext {
        self.apply_to_parse_context(&mut ctx);
        ctx
    }

    /// Layer experimental flags onto `ctx` (in-place; avoids repeated [`ParseContext::with_set`] moves).
    fn apply_to_parse_context(self, ctx: &mut ParseContext) {
        if self.let_bindings {
            ctx.set(FLAG_EXP_LET);
        }
        if self.lexical_const {
            ctx.set(FLAG_EXP_LEXICAL_CONST);
        }
        if self.match_stmt {
            ctx.set(FLAG_EXP_MATCH);
        }
        if self.modules {
            ctx.set(FLAG_EXP_MODULES);
        }
        if self.exceptions {
            ctx.set(FLAG_EXP_EXCEPTIONS);
        }
        if self.goto {
            ctx.set(FLAG_EXP_GOTO);
        }
        if self.loop_levels {
            ctx.set(FLAG_EXP_LOOP_LEVELS);
        }
        if self.fn_optional_params {
            ctx.set(FLAG_EXP_FN_OPTIONAL_PARAMS);
        }
        if self.templates {
            ctx.set(FLAG_EXP_TEMPLATES);
        }
    }
}

/// Dialect version plus experimental parse flags for the combined [`ParseContext`](sipha::prelude::ParseContext).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageOptions {
    pub version: Version,
    pub experimental: ExperimentalFeatures,
}

impl Default for LanguageOptions {
    fn default() -> Self {
        Self {
            version: Version::V4,
            experimental: ExperimentalFeatures::NONE,
        }
    }
}

impl LanguageOptions {
    #[must_use]
    pub fn new(version: Version, experimental: ExperimentalFeatures) -> Self {
        Self {
            version,
            experimental,
        }
    }

    /// LeekScript 4 with every experimental parse feature enabled (replaces the old `v-next` dialect).
    #[must_use]
    pub fn v4_experimental_all() -> Self {
        Self {
            version: Version::V4,
            experimental: ExperimentalFeatures::ALL,
        }
    }

    #[must_use]
    pub fn parse_context(self) -> ParseContext {
        let mut ctx = self.version.to_parse_context();
        self.experimental.apply_to_parse_context(&mut ctx);
        ctx
    }

    /// Parse context for signature / stub units (`function … => T;`) and merged `--signatures` checks.
    ///
    /// Includes [`FLAG_EXP_FN_OPTIONAL_PARAMS`] so stub parameters may use
    /// defaults (`integer depth = 1`) without enabling that experiment for normal [`parse_doc`] calls.
    ///
    /// Includes [`FLAG_EXP_TEMPLATES`] because API / stdlib stubs are overwhelmingly generic
    /// (`function mapValues<T, U>(…)`) and must parse as real top-level function declarations for scope analysis.
    #[must_use]
    pub fn signature_parse_context(self) -> ParseContext {
        let mut ctx = self.parse_context();
        ctx.set(FLAG_SIGNATURE_MODE);
        ctx.set(FLAG_EXP_FN_OPTIONAL_PARAMS);
        ctx.set(FLAG_EXP_TEMPLATES);
        ctx
    }
}

impl From<Version> for LanguageOptions {
    fn from(version: Version) -> Self {
        Self {
            version,
            experimental: ExperimentalFeatures::NONE,
        }
    }
}

/// Language dialect for parsing (v1–v4). Experimental syntax is enabled separately via
/// [`ExperimentalFeatures`] on [`LanguageOptions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    V1,
    V2,
    V3,
    V4,
}

impl Version {
    /// Parse a dialect label from `//! leeklang: version=…` / `dialect=…` directives (`v1`–`v4`, `1`–`4`, `ls1`–`ls4`).
    #[must_use]
    pub fn parse_dialect_label(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("v1") || t == "1" || t.eq_ignore_ascii_case("ls1") {
            Some(Self::V1)
        } else if t.eq_ignore_ascii_case("v2") || t == "2" || t.eq_ignore_ascii_case("ls2") {
            Some(Self::V2)
        } else if t.eq_ignore_ascii_case("v3") || t == "3" || t.eq_ignore_ascii_case("ls3") {
            Some(Self::V3)
        } else if t.eq_ignore_ascii_case("v4") || t == "4" || t.eq_ignore_ascii_case("ls4") {
            Some(Self::V4)
        } else {
            None
        }
    }

    /// Language levels are cumulative (v4 includes v3 and v2), except v1-only
    /// lexer/parser behavior which remains gated on `FLAG_V1` alone.
    #[must_use]
    pub fn to_parse_context(self) -> ParseContext {
        let mut ctx = ParseContext::with_capacity_for(FLAG_EXP_TEMPLATES);
        match self {
            Version::V1 => ctx.set(FLAG_V1),
            Version::V2 => ctx.set(FLAG_V2),
            Version::V3 => {
                ctx.set(FLAG_V2);
                ctx.set(FLAG_V3);
            }
            Version::V4 => {
                ctx.set(FLAG_V2);
                ctx.set(FLAG_V3);
                ctx.set(FLAG_V4);
            }
        }
        ctx
    }

    /// Same dialect as [`to_parse_context`](Self::to_parse_context), plus stub-style `function … => T;`
    /// declarations and default values on top-level function parameters (see [`LanguageOptions::signature_parse_context`]).
    #[must_use]
    pub fn to_signature_parse_context(self) -> ParseContext {
        let mut ctx = self.to_parse_context();
        ctx.set(FLAG_SIGNATURE_MODE);
        ctx.set(FLAG_EXP_FN_OPTIONAL_PARAMS);
        ctx.set(FLAG_EXP_TEMPLATES);
        ctx
    }
}
