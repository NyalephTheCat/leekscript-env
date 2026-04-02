//! Emits `GrammarBuilder` parse-context flag checks unless Cargo feature `grammar-v4-only` is on.
//!
//! With that feature, the built graph assumes [`Version::V4`](crate::parse::Version::V4); these
//! become no-ops so lexer/parser bytecode avoids runtime flag branches.

#[cfg(not(feature = "grammar-v4-only"))]
use crate::parse::version::{FLAG_V1, FLAG_V2, FLAG_V3, FLAG_V4};
use crate::parse::version::FLAG_VNEXT;
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

/// Upcoming-language features (`Version::VNext`). Unlike `v2` / `v3` / `v4`, this is **not**
/// stripped when `grammar-v4-only` is on, so V4 and VNext stay distinguishable at parse time.
pub(crate) fn vnext(g: &mut GrammarBuilder) {
    g.if_flag(FLAG_VNEXT);
}

pub(crate) fn not_vnext(g: &mut GrammarBuilder) {
    g.if_not_flag(FLAG_VNEXT);
}
