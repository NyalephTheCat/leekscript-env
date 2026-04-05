//! Shared [`sipha::prelude::GrammarBuilder`] settings for LeekScript.
//!
//! [`configure_builder`] enables Sipha’s bytecode optimizer ([`GrammarBuilder::set_optimize_graph`]),
//! which fuses ordered-choice spines into `ByteDispatch` instructions where possible. The parse VM
//! then uses staged opcode dispatch on the resulting graph.

use super::GRule;
use sipha::prelude::*;

/// Apply LeekScript-wide builder options (trivia, recursion, **graph optimize**).
#[inline]
pub(crate) fn configure_builder(g: &mut GrammarBuilder) {
    g.allow_rule_cycles(true);
    g.set_trivia_rule_name(GRule::Trivia);
    g.set_optimize_graph(true);
}
