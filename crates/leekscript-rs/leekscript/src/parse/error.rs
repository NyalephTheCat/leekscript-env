#[derive(Debug)]
pub enum ParseError {
    Sipha(ParseErrorInner),
    NoSyntaxRoot,
}

#[derive(Debug)]
pub enum ParseErrorInner {
    NoMatch(sipha::diagnostics::error::Diagnostic),
    Other(sipha::parse::engine::ParseError),
}

impl ParseError {
    #[must_use]
    pub fn format_with_source(&self, src: &str) -> String {
        match self {
            Self::Sipha(ParseErrorInner::NoMatch(d)) => {
                let idx = sipha::diagnostics::line_index::LineIndex::new(src.as_bytes());
                let graph = crate::grammar::built_graph().as_graph();
                d.format_with_source_deduped_expected(
                    src.as_bytes(),
                    &idx,
                    Some(&graph.literals),
                    Some(&graph),
                )
            }
            other => format!("{other:?}"),
        }
    }
}

impl From<sipha::parse::engine::ParseError> for ParseError {
    fn from(value: sipha::parse::engine::ParseError) -> Self {
        match value {
            sipha::parse::engine::ParseError::NoMatch(d) => {
                Self::Sipha(ParseErrorInner::NoMatch(d))
            }
            other => Self::Sipha(ParseErrorInner::Other(other)),
        }
    }
}
