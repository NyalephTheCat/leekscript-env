//! Plain `keyword(...)` lexer rules gated by v3 / v4 / experimental flags, registered with sipha
//! [`LexerKeywordSpec`](sipha::parse::lexer_batch::LexerKeywordSpec) /
//! [`GrammarBuilder::lexer_rule_keywords_batch`](sipha::prelude::GrammarBuilder::lexer_rule_keywords_batch).
//!
//! Tables are split to match **registration order** in [`super::tokens::define_lexer_keywords`]
//! (versioned / v2 rules sit between these batches). `integer` stays before `int`; `kw_in` splits
//! the Java reserved block.

use super::GRule;
use crate::syntax::kinds::Lex;
use sipha::parse::context::FlagId;
use sipha::parse::lexer_batch::LexerKeywordSpec;

mod flags {
    use super::FlagId;
    use crate::parse::version::{
        FLAG_EXP_EXCEPTIONS, FLAG_EXP_GOTO, FLAG_EXP_LEXICAL_CONST, FLAG_EXP_LET, FLAG_EXP_MATCH,
        FLAG_EXP_MODULES, FLAG_V3, FLAG_V4,
    };

    pub const V3: &[FlagId] = &[FLAG_V3];
    pub const LET: &[FlagId] = &[FLAG_V3, FLAG_EXP_LET];
    pub const MATCH: &[FlagId] = &[FLAG_V4, FLAG_EXP_MATCH];
    pub const V3_EXC: &[FlagId] = &[FLAG_V3, FLAG_EXP_EXCEPTIONS];
    pub const V3_CONST: &[FlagId] = &[FLAG_V3, FLAG_EXP_LEXICAL_CONST];
    pub const V3_MOD: &[FlagId] = &[FLAG_V3, FLAG_EXP_MODULES];
    pub const V3_GOTO: &[FlagId] = &[FLAG_V3, FLAG_EXP_GOTO];
}

pub(super) const KW_LET: &[LexerKeywordSpec<Lex>] = &[LexerKeywordSpec::new(GRule::KwLet.as_str(), Lex::LetKw, b"let")
    .with_flags(flags::LET)];

pub(super) const KW_MATCH: &[LexerKeywordSpec<Lex>] = &[LexerKeywordSpec::new(GRule::KwMatch.as_str(),
    Lex::MatchKw,
    b"match",
)
.with_flags(flags::MATCH)];

pub(super) const PLAIN_V3_SWITCH: &[LexerKeywordSpec<Lex>] = &[
    LexerKeywordSpec::new(GRule::KwSwitch.as_str(), Lex::SwitchKw, b"switch").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwCase.as_str(), Lex::CaseKw, b"case").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwDefault.as_str(), Lex::DefaultKw, b"default").with_flags(flags::V3),
];

pub(super) const PLAIN_V3_FINAL: &[LexerKeywordSpec<Lex>] =
    &[LexerKeywordSpec::new(GRule::KwFinal.as_str(), Lex::FinalKw, b"final").with_flags(flags::V3)];

pub(super) const PLAIN_V3_TYPE_NAMES: &[LexerKeywordSpec<Lex>] = &[
    LexerKeywordSpec::new(GRule::KwVoid.as_str(), Lex::VoidKw, b"void").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwBoolean.as_str(), Lex::BooleanKw, b"boolean").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwAny.as_str(), Lex::AnyKw, b"any").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwInteger.as_str(), Lex::IntegerKw, b"integer").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwInt.as_str(), Lex::IntKw, b"int").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwReal.as_str(), Lex::RealKw, b"real").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwStringType.as_str(), Lex::StringTypeKw, b"string").with_flags(flags::V3),
];

pub(super) const PLAIN_V3_JAVA_BEFORE_KW_IN: &[LexerKeywordSpec<Lex>] = &[
    LexerKeywordSpec::new(GRule::KwAbstract.as_str(), Lex::AbstractKw, b"abstract").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwAwait.as_str(), Lex::AwaitKw, b"await").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwByte.as_str(), Lex::ByteKw, b"byte").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwCatch.as_str(), Lex::CatchKw, b"catch").with_flags(flags::V3_EXC),
    LexerKeywordSpec::new(GRule::KwChar.as_str(), Lex::CharKw, b"char").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwConst.as_str(), Lex::ConstKw, b"const").with_flags(flags::V3_CONST),
    LexerKeywordSpec::new(GRule::KwDouble.as_str(), Lex::DoubleKw, b"double").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwEnum.as_str(), Lex::EnumKw, b"enum").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwEval.as_str(), Lex::EvalKw, b"eval").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwExport.as_str(), Lex::ExportKw, b"export").with_flags(flags::V3_MOD),
    LexerKeywordSpec::new(GRule::KwFinally.as_str(), Lex::FinallyKw, b"finally").with_flags(flags::V3_EXC),
    LexerKeywordSpec::new(GRule::KwFloat.as_str(), Lex::FloatKw, b"float").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwGoto.as_str(), Lex::GotoKw, b"goto").with_flags(flags::V3_GOTO),
    LexerKeywordSpec::new(GRule::KwImplements.as_str(), Lex::ImplementsKw, b"implements").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwImport.as_str(), Lex::ImportKw, b"import").with_flags(flags::V3_MOD),
    LexerKeywordSpec::new(GRule::KwInterface.as_str(), Lex::InterfaceKw, b"interface").with_flags(flags::V3),
];

pub(super) const PLAIN_V3_JAVA_AFTER_KW_IN: &[LexerKeywordSpec<Lex>] = &[
    LexerKeywordSpec::new(GRule::KwLong.as_str(), Lex::LongKw, b"long").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwNative.as_str(), Lex::NativeKw, b"native").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwPackage.as_str(), Lex::PackageKw, b"package").with_flags(flags::V3_MOD),
    LexerKeywordSpec::new(GRule::KwShort.as_str(), Lex::ShortKw, b"short").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwSynchronized.as_str(), Lex::SynchronizedKw, b"synchronized").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwThrow.as_str(), Lex::ThrowKw, b"throw").with_flags(flags::V3_EXC),
    LexerKeywordSpec::new(GRule::KwThrows.as_str(), Lex::ThrowsKw, b"throws").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwTransient.as_str(), Lex::TransientKw, b"transient").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwTry.as_str(), Lex::TryKw, b"try").with_flags(flags::V3_EXC),
    LexerKeywordSpec::new(GRule::KwTypeof.as_str(), Lex::TypeofKw, b"typeof").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwVolatile.as_str(), Lex::VolatileKw, b"volatile").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwWith.as_str(), Lex::WithKw, b"with").with_flags(flags::V3),
    LexerKeywordSpec::new(GRule::KwYield.as_str(), Lex::YieldKw, b"yield").with_flags(flags::V3),
];
