//! User-facing knobs for [`super::format_document`](super::format_document).

/// How `{` is placed relative to the header (`function foo()` / `if (...)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BraceStyle {
    /// `function foo() {` — opening brace on the same line as the header.
    #[default]
    SameLine,
    /// Allman-style: brace on its own line (only applied where the printer has explicit block layout).
    NextLine,
}

/// Line ending used for newly inserted line breaks (verbatim regions keep source bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    Crlf,
}

impl LineEnding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

/// How statement-ending semicolons are printed when the grammar treats them as optional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SemicolonStyle {
    /// Keep semicolons exactly as in the parse tree (omit none, add none).
    #[default]
    Preserve,
    /// Always end eligible statements with `;` (insert when missing).
    Always,
    /// Only keep or insert `;` where it is required for readability — bare `return`, `break`, and
    /// `continue` (with optional level); drop optional semicolons elsewhere (`let x = 1`, `return y`).
    OnlyNeeded,
}

impl SemicolonStyle {
    /// Parse from directive / config text (`preserve`, `always`, `only-needed`, …).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let v = s.trim().to_ascii_lowercase().replace('-', "_");
        match v.as_str() {
            "preserve" => Some(Self::Preserve),
            "always" => Some(Self::Always),
            "onlyneeded" | "only_needed" => Some(Self::OnlyNeeded),
            _ => None,
        }
    }
}

/// All formatter settings in one place. Clone this when entering nested scopes.
#[derive(Clone, Debug, PartialEq)]
pub struct FormatOptions {
    /// Spaces per indent level when [`Self::use_tabs`] is false.
    pub indent_width: usize,
    /// Use tab (`\t`) for each indent level instead of spaces.
    pub use_tabs: bool,
    /// Logical tab width (indentation display width and line wrapping).
    pub tab_width: usize,
    /// Max line length before comma-triggered wraps (`0` = unlimited).
    pub line_width: usize,
    pub brace_style: BraceStyle,
    /// Blank lines between top-level statements (`0` = single newline only).
    pub blank_lines_between_top_level: usize,
    /// Extra blank lines after a top-level [`crate::syntax::kinds::K::ClassDecl`] before the next
    /// top-level item (`0` = no extra beyond [`Self::blank_lines_between_top_level`]).
    pub blank_lines_after_class: usize,
    /// `if (` vs `if(` for `if`, `while`, `for`, `switch`, `catch`, etc.
    pub space_after_keyword_before_paren: bool,
    /// `function foo (` vs `function foo(`.
    pub space_before_function_decl_paren: bool,
    /// `( x )` vs `(x)` for parenthesized groups the printer lays out.
    pub space_inside_parens: bool,
    /// `x = 1` vs `x=1` for `=` in declarations and assignments.
    pub space_around_assign: bool,
    /// `a + b` vs `a+b` for binary operators (also `==`, `<`, etc.).
    pub space_around_binary_ops: bool,
    /// `a, b` vs `a,b` after commas in lists and parameter lists.
    pub space_after_comma: bool,
    /// Spaces around `|`, `<`, `>` inside type / generic syntax (`integer | real`, `Array < number >`).
    /// When `false`, types use compact punctuation (`integer|real`, `Array<number>`).
    pub space_around_type_operators: bool,
    /// Insert a line break before `else` / `catch` / `finally` when they follow a closing `}`.
    pub newline_before_else_catch_finally: bool,
    /// Force a trailing newline at end of output.
    pub trailing_newline: bool,
    /// Extra blank lines between consecutive statements inside `{ ... }` (`0` = single newline only).
    pub blank_lines_between_block_statements: usize,
    /// Extra blank lines between class members in a class body (`0` = single newline only).
    /// Consecutive **fields** (members without a `{ ... }` body) stay adjacent; this still applies
    /// between methods and between fields and methods.
    /// The class `{ ... }` block is the direct child of [`crate::syntax::kinds::K::ClassDecl`].
    pub blank_lines_between_class_members: usize,
    /// When non-zero, caps [`Self::blank_lines_between_block_statements`],
    /// [`Self::blank_lines_between_class_members`], and **source-preserved** blank lines between
    /// block/class members (`0` = no cap on policy; source gaps are still limited to 10).
    pub max_consecutive_blank_lines_in_block: usize,
    /// Line endings for inserted breaks (verbatim regions keep original bytes).
    pub line_ending: LineEnding,
    /// Optional statement terminators (`return;` vs `return`, `let x = 1;` vs `let x = 1`, …).
    pub semicolon_style: SemicolonStyle,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 4,
            use_tabs: false,
            tab_width: 4,
            line_width: 100,
            brace_style: BraceStyle::default(),
            blank_lines_between_top_level: 0,
            blank_lines_after_class: 2,
            space_after_keyword_before_paren: true,
            space_before_function_decl_paren: false,
            space_inside_parens: false,
            space_around_assign: true,
            space_around_binary_ops: true,
            space_after_comma: true,
            space_around_type_operators: false,
            newline_before_else_catch_finally: false,
            trailing_newline: true,
            blank_lines_between_block_statements: 0,
            blank_lines_between_class_members: 1,
            max_consecutive_blank_lines_in_block: 2,
            line_ending: LineEnding::default(),
            semicolon_style: SemicolonStyle::default(),
        }
    }
}

impl FormatOptions {
    /// One indent unit as a string (either `"\t"` or N spaces).
    #[must_use]
    pub fn indent_unit(&self) -> String {
        if self.use_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.indent_width)
        }
    }

    /// Apply non-`None` fields from `patch` (from `leekfmt:` directives, including file-wide `//!`).
    pub fn apply_patch(&mut self, patch: &FormatPatch) {
        if let Some(v) = patch.indent_width {
            self.indent_width = v;
        }
        if let Some(v) = patch.use_tabs {
            self.use_tabs = v;
        }
        if let Some(v) = patch.tab_width {
            self.tab_width = v;
        }
        if let Some(v) = patch.line_width {
            self.line_width = v;
        }
        if let Some(v) = patch.brace_style {
            self.brace_style = v;
        }
        if let Some(v) = patch.blank_lines_between_top_level {
            self.blank_lines_between_top_level = v;
        }
        if let Some(v) = patch.blank_lines_after_class {
            self.blank_lines_after_class = v;
        }
        if let Some(v) = patch.space_after_keyword_before_paren {
            self.space_after_keyword_before_paren = v;
        }
        if let Some(v) = patch.space_before_function_decl_paren {
            self.space_before_function_decl_paren = v;
        }
        if let Some(v) = patch.space_inside_parens {
            self.space_inside_parens = v;
        }
        if let Some(v) = patch.space_around_assign {
            self.space_around_assign = v;
        }
        if let Some(v) = patch.space_around_binary_ops {
            self.space_around_binary_ops = v;
        }
        if let Some(v) = patch.space_after_comma {
            self.space_after_comma = v;
        }
        if let Some(v) = patch.space_around_type_operators {
            self.space_around_type_operators = v;
        }
        if let Some(v) = patch.newline_before_else_catch_finally {
            self.newline_before_else_catch_finally = v;
        }
        if let Some(v) = patch.trailing_newline {
            self.trailing_newline = v;
        }
        if let Some(v) = patch.blank_lines_between_block_statements {
            self.blank_lines_between_block_statements = v;
        }
        if let Some(v) = patch.blank_lines_between_class_members {
            self.blank_lines_between_class_members = v;
        }
        if let Some(v) = patch.max_consecutive_blank_lines_in_block {
            self.max_consecutive_blank_lines_in_block = v;
        }
        if let Some(v) = patch.line_ending {
            self.line_ending = v;
        }
        if let Some(v) = patch.semicolon_style {
            self.semicolon_style = v;
        }
    }

    /// Options in effect at `byte_offset` in the original source after applying ordered patches.
    #[must_use]
    pub fn effective_at(base: &Self, patches: &[(u32, FormatPatch)], byte_offset: u32) -> Self {
        let mut o = base.clone();
        for (pos, p) in patches {
            if *pos <= byte_offset {
                o.apply_patch(p);
            }
        }
        o
    }
}

/// Subset of [`FormatOptions`] for directive overrides (`None` = leave unchanged).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormatPatch {
    pub indent_width: Option<usize>,
    pub use_tabs: Option<bool>,
    pub tab_width: Option<usize>,
    pub line_width: Option<usize>,
    pub brace_style: Option<BraceStyle>,
    pub blank_lines_between_top_level: Option<usize>,
    pub blank_lines_after_class: Option<usize>,
    pub space_after_keyword_before_paren: Option<bool>,
    pub space_before_function_decl_paren: Option<bool>,
    pub space_inside_parens: Option<bool>,
    pub space_around_assign: Option<bool>,
    pub space_around_binary_ops: Option<bool>,
    pub space_after_comma: Option<bool>,
    pub space_around_type_operators: Option<bool>,
    pub newline_before_else_catch_finally: Option<bool>,
    pub trailing_newline: Option<bool>,
    pub blank_lines_between_block_statements: Option<usize>,
    pub blank_lines_between_class_members: Option<usize>,
    pub max_consecutive_blank_lines_in_block: Option<usize>,
    pub line_ending: Option<LineEnding>,
    pub semicolon_style: Option<SemicolonStyle>,
}
