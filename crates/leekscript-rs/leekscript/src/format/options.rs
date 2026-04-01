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

/// All formatter settings in one place. Clone this when entering nested scopes.
#[derive(Clone, Debug, PartialEq)]
pub struct FormatOptions {
    /// Spaces per indent level when [`Self::use_tabs`] is false.
    pub indent_width: usize,
    /// Use tab (`\t`) for each indent level instead of spaces.
    pub use_tabs: bool,
    /// Logical tab width (for line-width accounting only; reserved for future wrapping).
    pub tab_width: usize,
    pub line_width: usize,
    pub brace_style: BraceStyle,
    /// Blank lines between top-level statements (`0` = single newline only).
    pub blank_lines_between_top_level: usize,
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
    /// Insert a line break before `else` / `catch` / `finally` when they follow a closing `}`.
    pub newline_before_else_catch_finally: bool,
    /// Force a trailing newline at end of output.
    pub trailing_newline: bool,
    /// Cap consecutive blank lines inside blocks (0 = unlimited).
    pub max_consecutive_blank_lines_in_block: usize,
    /// Line endings for inserted breaks (verbatim regions keep original bytes).
    pub line_ending: LineEnding,
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
            space_after_keyword_before_paren: true,
            space_before_function_decl_paren: false,
            space_inside_parens: false,
            space_around_assign: true,
            space_around_binary_ops: true,
            newline_before_else_catch_finally: false,
            trailing_newline: true,
            max_consecutive_blank_lines_in_block: 0,
            line_ending: LineEnding::default(),
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

    /// Apply non-`None` fields from `patch` (from `// leekfmt:` directives).
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
        if let Some(v) = patch.newline_before_else_catch_finally {
            self.newline_before_else_catch_finally = v;
        }
        if let Some(v) = patch.trailing_newline {
            self.trailing_newline = v;
        }
        if let Some(v) = patch.max_consecutive_blank_lines_in_block {
            self.max_consecutive_blank_lines_in_block = v;
        }
        if let Some(v) = patch.line_ending {
            self.line_ending = v;
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
    pub space_after_keyword_before_paren: Option<bool>,
    pub space_before_function_decl_paren: Option<bool>,
    pub space_inside_parens: Option<bool>,
    pub space_around_assign: Option<bool>,
    pub space_around_binary_ops: Option<bool>,
    pub newline_before_else_catch_finally: Option<bool>,
    pub trailing_newline: Option<bool>,
    pub max_consecutive_blank_lines_in_block: Option<usize>,
    pub line_ending: Option<LineEnding>,
}
