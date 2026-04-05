//! Declarative `macro_rules!` helpers for lexer rules. Expansion order matches invocation order;
//! do not reorder registrations without checking sipha trie / longest-match behavior.

macro_rules! lexer_kw_versioned {
    ($g:expr; $(($rule:path, $kind:expr, $spell:expr)),* $(,)?) => {
        $(crate::grammar::lexer_rules::versioned_keyword_rule($g, $rule.as_str(), $kind, $spell);)*
    };
}

macro_rules! lexer_kw_v2_if {
    ($g:expr; $(($rule:path, $kind:expr, $spell:expr)),* $(,)?) => {
        $(crate::grammar::lexer_rules::versioned_keyword_rule_if(
            $g,
            $rule.as_str(),
            crate::parse::version::FLAG_V2,
            $kind,
            $spell,
        );)*
    };
}

macro_rules! lexer_token_literal {
    ($g:expr; $(($rule:path, $kind:expr, $spell:expr)),* $(,)?) => {
        $(crate::grammar::lexer_rules::token_literal_rule($g, $rule.as_str(), $kind, $spell);)*
    };
}

macro_rules! lexer_token_byte {
    ($g:expr; $(($rule:path, $kind:expr, $byte:expr)),* $(,)?) => {
        $(crate::grammar::lexer_rules::token_byte_rule($g, $rule.as_str(), $kind, $byte);)*
    };
}
