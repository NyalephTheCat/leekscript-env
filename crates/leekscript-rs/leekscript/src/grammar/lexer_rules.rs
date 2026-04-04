//! Small helpers for registering repetitive `GrammarBuilder::lexer_rule` patterns.

#[cfg(not(feature = "grammar-v4-only"))]
use crate::parse::version::FLAG_V3;
use crate::syntax::kinds::K;
use sipha::prelude::*;

/// Reference lexer `wordEquals`: ASCII case-folding for language version <= 2.
#[cfg(not(feature = "grammar-v4-only"))]
pub(crate) fn ascii_ci_bytes(g: &mut GrammarBuilder, word: &[u8]) {
    for &b in word {
        if b.is_ascii_lowercase() {
            g.choice(
                |g| {
                    g.byte(b);
                },
                |g| {
                    g.byte(b.to_ascii_uppercase());
                },
            );
        } else if b.is_ascii_uppercase() {
            g.choice(
                |g| {
                    g.byte(b);
                },
                |g| {
                    g.byte(b.to_ascii_lowercase());
                },
            );
        } else {
            g.byte(b);
        }
    }
}

/// Case-sensitive keywords for v3+ (`FLAG_V3`), case-insensitive for v1/v2 (reference lexer).
pub(crate) fn versioned_keyword(g: &mut GrammarBuilder, kind: K, word: &'static [u8]) {
    #[cfg(feature = "grammar-v4-only")]
    {
        g.keyword(kind, word);
    }
    #[cfg(not(feature = "grammar-v4-only"))]
    {
        g.choice(
            |g| {
                g.if_flag(FLAG_V3);
                g.keyword(kind, word);
            },
            |g| {
                g.if_not_flag(FLAG_V3);
                g.token(kind, |g| {
                    ascii_ci_bytes(g, word);
                });
            },
        );
    }
}

/// `lexer_rule(name)` whose body is [`versioned_keyword`].
pub(crate) fn versioned_keyword_rule(
    g: &mut GrammarBuilder,
    name: &'static str,
    kind: K,
    word: &'static [u8],
) {
    g.lexer_rule(name, |g| {
        versioned_keyword(g, kind, word);
    });
}

/// `lexer_rule(name)` with `if_flag(flag)` then `keyword`.
pub(crate) fn keyword_rule_if(
    g: &mut GrammarBuilder,
    name: &'static str,
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] flag: FlagId,
    kind: K,
    word: &'static [u8],
) {
    g.lexer_rule(name, |g| {
        #[cfg(not(feature = "grammar-v4-only"))]
        {
            g.if_flag(flag);
        }
        g.keyword(kind, word);
    });
}

/// `lexer_rule(name)` with `if_flag(flag)` then [`versioned_keyword`].
pub(crate) fn versioned_keyword_rule_if(
    g: &mut GrammarBuilder,
    name: &'static str,
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] flag: FlagId,
    kind: K,
    word: &'static [u8],
) {
    g.lexer_rule(name, |g| {
        #[cfg(not(feature = "grammar-v4-only"))]
        {
            g.if_flag(flag);
        }
        versioned_keyword(g, kind, word);
    });
}

/// Keyword when `feature_flag` is set. When `grammar-v4-only` is off, also require `base_flag`
/// (for example `FLAG_V3` or `FLAG_V4`) so the spelling stays an identifier below that level.
pub(crate) fn keyword_rule_if_experimental(
    g: &mut GrammarBuilder,
    name: &'static str,
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] base_flag: FlagId,
    feature_flag: FlagId,
    kind: K,
    word: &'static [u8],
) {
    g.lexer_rule(name, |g| {
        #[cfg(not(feature = "grammar-v4-only"))]
        {
            g.if_flag(base_flag);
        }
        g.if_flag(feature_flag);
        g.keyword(kind, word);
    });
}

/// `lexer_rule(name)` with `token(kind, literal(bytes))`.
pub(crate) fn token_literal_rule(
    g: &mut GrammarBuilder,
    name: &'static str,
    kind: K,
    lit: &'static [u8],
) {
    g.lexer_rule(name, |g| {
        g.token(kind, |g| {
            g.literal(lit);
        });
    });
}

/// `lexer_rule(name)` with `token(kind, byte(b))`.
pub(crate) fn token_byte_rule(g: &mut GrammarBuilder, name: &'static str, kind: K, byte: u8) {
    g.lexer_rule(name, |g| {
        g.token(kind, |g| {
            g.byte(byte);
        });
    });
}
