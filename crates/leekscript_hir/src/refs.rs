//! Java `reference` ids emitted by [`crate::lower::lower_file`] — keep in sync with `lower/`.

/// Sorted, deduplicated list for [`leekscript_diagnostics::Registry`] checks.
pub const EMITTED_REFERENCES: &[&str] = &[
    "END_OF_INSTRUCTION_EXPECTED",
    "FUNCTION_NAME_EXPECTED",
    "INTERNAL_ERROR",
    "INVALID_NUMBER",
    "INVALID_OPERATOR",
    "KEYWORD_UNEXPECTED",
    "OPENING_PARENTHESIS_EXPECTED",
    "PARAMETER_NAME_EXPECTED",
    "UNCOMPLETE_EXPRESSION",
    "VALUE_EXPECTED",
    "VAR_NAME_EXPECTED",
];
