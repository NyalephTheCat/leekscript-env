//! Emits `GrammarBuilder` parse-context flag checks unless Cargo feature `grammar-v4-only` is on.
//!
//! With that feature, the built graph assumes [`Version::V4`](crate::parse::Version::V4); these
//! become no-ops so lexer/parser bytecode avoids runtime flag branches.

#[cfg(not(feature = "grammar-v4-only"))]
use crate::parse::version::{FLAG_V1, FLAG_V2, FLAG_V3, FLAG_V4};
use sipha::prelude::*;

pub(crate) fn v2(
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] g: &mut GrammarBuilder,
) {
    #[cfg(not(feature = "grammar-v4-only"))]
    g.if_flag(FLAG_V2);
}

pub(crate) fn v3(
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] g: &mut GrammarBuilder,
) {
    #[cfg(not(feature = "grammar-v4-only"))]
    g.if_flag(FLAG_V3);
}

pub(crate) fn v4(
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] g: &mut GrammarBuilder,
) {
    #[cfg(not(feature = "grammar-v4-only"))]
    g.if_flag(FLAG_V4);
}

pub(crate) fn not_v1(
    #[cfg_attr(feature = "grammar-v4-only", allow(unused_variables))] g: &mut GrammarBuilder,
) {
    #[cfg(not(feature = "grammar-v4-only"))]
    g.if_not_flag(FLAG_V1);
}
