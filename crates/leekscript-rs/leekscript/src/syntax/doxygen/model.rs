//! Data structures for parsed Doxygen comments.

/// `\param` / `\tparam` / `@param` — optional `[in]` / `[out]` / `[in,out]`, name, description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoxygenParam {
    pub direction: Option<String>,
    pub name: String,
    pub description: String,
}

/// `\throws` / `\exception` / `@throws` — optional type word, then description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoxygenThrows {
    pub type_name: Option<String>,
    pub description: String,
}

/// `\retval value description` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoxygenRetval {
    pub value: String,
    pub description: String,
}

/// Structured Doxygen body for a declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDoxygen {
    /// Full normalized comment text (same as pre-split `attached_docstring` output).
    pub raw: String,
    pub brief: Option<String>,
    pub details: Option<String>,
    pub params: Vec<DoxygenParam>,
    /// Template parameters (`\tparam`, `\template`).
    pub template_params: Vec<DoxygenParam>,
    pub returns: Option<String>,
    /// `\retval` entries.
    pub retvals: Vec<DoxygenRetval>,
    pub see_also: Vec<String>,
    pub deprecated: Option<String>,
    pub note: Option<String>,
    pub warning: Option<String>,
    pub attention: Option<String>,
    pub preconditions: Option<String>,
    pub postconditions: Option<String>,
    pub invariant: Option<String>,
    pub remark: Option<String>,
    pub throws: Vec<DoxygenThrows>,
    pub since: Option<String>,
    pub authors: Vec<String>,
    pub version: Option<String>,
    pub copyright: Option<String>,
    pub bugs: Vec<String>,
    pub todos: Vec<String>,
    pub tests: Vec<String>,
    /// `\internal` present (no argument required).
    pub internal: bool,
    /// `\overload` present.
    pub overload: bool,
    /// Commands not mapped to a dedicated field (`\fn`, `\file`, …).
    pub unknown: Vec<(String, String)>,
}

impl Default for ParsedDoxygen {
    fn default() -> Self {
        Self {
            raw: String::new(),
            brief: None,
            details: None,
            params: Vec::new(),
            template_params: Vec::new(),
            returns: None,
            retvals: Vec::new(),
            see_also: Vec::new(),
            deprecated: None,
            note: None,
            warning: None,
            attention: None,
            preconditions: None,
            postconditions: None,
            invariant: None,
            remark: None,
            throws: Vec::new(),
            since: None,
            authors: Vec::new(),
            version: None,
            copyright: None,
            bugs: Vec::new(),
            todos: Vec::new(),
            tests: Vec::new(),
            internal: false,
            overload: false,
            unknown: Vec::new(),
        }
    }
}

impl ParsedDoxygen {
    /// Plain-text summary: [`Self::brief`] or first line of raw body.
    #[must_use]
    pub fn summary(&self) -> Option<&str> {
        self.brief
            .as_deref()
            .or_else(|| self.raw.lines().next().map(str::trim))
            .filter(|s| !s.is_empty())
    }
}
