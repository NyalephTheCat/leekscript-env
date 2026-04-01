use sipha::prelude::*;

pub const FLAG_V1: FlagId = 1;
pub const FLAG_V2: FlagId = 2;
pub const FLAG_V3: FlagId = 3;
pub const FLAG_V4: FlagId = 4;

/// Language dialect for parsing.
///
/// When the `grammar-v4-only` Cargo feature is enabled on the `leekscript` crate, only
/// [`Version::V4`] matches the compiled grammar; do not use [`Version::V1`], [`Version::V2`], or
/// [`Version::V3`] in that configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    V1,
    V2,
    V3,
    V4,
}

impl Version {
    /// Language levels are cumulative (v4 includes v3 and v2), except v1-only
    /// lexer/parser behavior which remains gated on `FLAG_V1` alone.
    #[must_use]
    pub fn to_parse_context(self) -> ParseContext {
        match self {
            Version::V1 => ParseContext::new().with_set(FLAG_V1),
            Version::V2 => ParseContext::new().with_set(FLAG_V2),
            Version::V3 => ParseContext::new().with_set(FLAG_V2).with_set(FLAG_V3),
            Version::V4 => ParseContext::new()
                .with_set(FLAG_V2)
                .with_set(FLAG_V3)
                .with_set(FLAG_V4),
        }
    }
}
