pub mod kinds;

use crate::syntax::kinds::K;
use sipha::tree::red::SyntaxElement;

/// True for whitespace / comment tokens and for the lexer’s grouped [`K::Trivia`] node (it only
/// contains trivia token leaves).
#[inline]
pub(crate) fn syntax_el_is_trivia(el: &SyntaxElement) -> bool {
    el.is_trivia() || el.kind_as::<K>() == Some(K::Trivia)
}
