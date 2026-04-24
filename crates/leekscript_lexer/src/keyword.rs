//! Map identifier words to keywords / word-operators (Java `LexicalParser.tryParseIdentifier`).

use crate::token::TokenKind;

fn word_eq(word: &str, expected: &str, version: u8) -> bool {
    if version <= 2 {
        word.eq_ignore_ascii_case(expected)
    } else {
        word == expected
    }
}

/// Classify a raw identifier `word` (source slice). Returns [`TokenKind::Ident`] if not reserved.
pub fn classify_word(word: &str, version: u8) -> TokenKind {
    // `and` / `or` / `xor` / `instanceof` → operator tokens in Java
    if word_eq(word, "and", version) {
        return TokenKind::WordOp(WordOp::And);
    }
    if word_eq(word, "or", version) {
        return TokenKind::WordOp(WordOp::Or);
    }
    if word_eq(word, "xor", version) {
        return TokenKind::WordOp(WordOp::Xor);
    }
    if version >= 2 && word_eq(word, "instanceof", version) {
        return TokenKind::WordOp(WordOp::Instanceof);
    }
    // Java test suite uses `is` / `is not` as equality sugar in all versions.
    if word_eq(word, "is", version) {
        return TokenKind::WordOp(WordOp::Is);
    }

    if word_eq(word, "as", version) {
        return TokenKind::Kw(Kw::As);
    }
    if word_eq(word, "var", version) {
        return TokenKind::Kw(Kw::Var);
    }
    if word_eq(word, "global", version) {
        return TokenKind::Kw(Kw::Global);
    }
    if word_eq(word, "return", version) {
        return TokenKind::Kw(Kw::Return);
    }
    if version >= 2 && word_eq(word, "constructor", version) {
        return TokenKind::Kw(Kw::Constructor);
    }
    if version >= 2 && word_eq(word, "final", version) {
        return TokenKind::Kw(Kw::Final);
    }
    if word_eq(word, "for", version) {
        return TokenKind::Kw(Kw::For);
    }
    if word_eq(word, "if", version) {
        return TokenKind::Kw(Kw::If);
    }
    if word_eq(word, "while", version) {
        return TokenKind::Kw(Kw::While);
    }
    if version >= 2 && word_eq(word, "static", version) {
        return TokenKind::Kw(Kw::Static);
    }
    if word_eq(word, "in", version) {
        return TokenKind::Kw(Kw::In);
    }
    if version >= 3 && word_eq(word, "abstract", version) {
        return TokenKind::Kw(Kw::Abstract);
    }
    if version >= 3 && word_eq(word, "await", version) {
        return TokenKind::Kw(Kw::Await);
    }
    if word_eq(word, "break", version) {
        return TokenKind::Kw(Kw::Break);
    }
    if word_eq(word, "continue", version) {
        return TokenKind::Kw(Kw::Continue);
    }
    if version >= 3 && word_eq(word, "import", version) {
        return TokenKind::Kw(Kw::Import);
    }
    if version >= 3 && word_eq(word, "export", version) {
        return TokenKind::Kw(Kw::Export);
    }
    if version >= 3 && word_eq(word, "goto", version) {
        return TokenKind::Kw(Kw::Goto);
    }
    if version >= 3 && word_eq(word, "switch", version) {
        return TokenKind::Kw(Kw::Switch);
    }
    if version >= 2 && word_eq(word, "super", version) {
        return TokenKind::Kw(Kw::Super);
    }
    // Java uses exact `word.equals("class")` (case-sensitive) when version >= 2
    if version >= 2 && word == "class" {
        return TokenKind::Kw(Kw::Class);
    }
    if version >= 3 && word_eq(word, "catch", version) {
        return TokenKind::Kw(Kw::Catch);
    }
    if version >= 2 && word_eq(word, "extends", version) {
        return TokenKind::Kw(Kw::Extends);
    }
    if word_eq(word, "true", version) {
        return TokenKind::Kw(Kw::True);
    }
    if word_eq(word, "false", version) {
        return TokenKind::Kw(Kw::False);
    }
    if version >= 3 && word_eq(word, "const", version) {
        return TokenKind::Kw(Kw::Const);
    }
    if version >= 3 && word_eq(word, "char", version) {
        return TokenKind::Kw(Kw::Char);
    }
    if version >= 3 && word_eq(word, "enum", version) {
        return TokenKind::Kw(Kw::Enum);
    }
    if version >= 3 && word_eq(word, "eval", version) {
        return TokenKind::Kw(Kw::Eval);
    }
    if version >= 3 && word_eq(word, "case", version) {
        return TokenKind::Kw(Kw::Case);
    }
    if version >= 3 && word_eq(word, "float", version) {
        return TokenKind::Kw(Kw::Float);
    }
    if version >= 3 && word_eq(word, "double", version) {
        return TokenKind::Kw(Kw::Double);
    }
    if version >= 3 && word_eq(word, "byte", version) {
        return TokenKind::Kw(Kw::Byte);
    }
    if word_eq(word, "do", version) {
        return TokenKind::Kw(Kw::Do);
    }
    if version >= 3 && word_eq(word, "try", version) {
        return TokenKind::Kw(Kw::Try);
    }
    // Java suite uses `void` in type positions across versions.
    if word_eq(word, "void", version) {
        return TokenKind::Kw(Kw::Void);
    }
    if version >= 3 && word_eq(word, "with", version) {
        return TokenKind::Kw(Kw::With);
    }
    if version >= 3 && word_eq(word, "yield", version) {
        return TokenKind::Kw(Kw::Yield);
    }
    if version >= 3 && word_eq(word, "finally", version) {
        return TokenKind::Kw(Kw::Finally);
    }
    if version >= 3 && word_eq(word, "interface", version) {
        return TokenKind::Kw(Kw::Interface);
    }
    if version >= 3 && word_eq(word, "long", version) {
        return TokenKind::Kw(Kw::Long);
    }
    if version >= 3 && word_eq(word, "let", version) {
        return TokenKind::Kw(Kw::Let);
    }
    if version >= 3 && word_eq(word, "native", version) {
        return TokenKind::Kw(Kw::Native);
    }
    if version >= 2 && word_eq(word, "new", version) {
        return TokenKind::Kw(Kw::New);
    }
    if version >= 3 && word_eq(word, "package", version) {
        return TokenKind::Kw(Kw::Package);
    }
    if version >= 2 && word_eq(word, "this", version) {
        return TokenKind::Kw(Kw::This);
    }
    if word_eq(word, "function", version) {
        return TokenKind::Kw(Kw::Function);
    }
    if version >= 3 && word_eq(word, "implements", version) {
        return TokenKind::Kw(Kw::Implements);
    }
    if version >= 3 && word_eq(word, "int", version) {
        return TokenKind::Kw(Kw::Int);
    }
    if word_eq(word, "not", version) {
        return TokenKind::Kw(Kw::Not);
    }
    if word_eq(word, "null", version) {
        return TokenKind::Kw(Kw::Null);
    }
    if version >= 2 && word_eq(word, "private", version) {
        return TokenKind::Kw(Kw::Private);
    }
    if version >= 2 && word_eq(word, "protected", version) {
        return TokenKind::Kw(Kw::Protected);
    }
    if version >= 2 && word_eq(word, "public", version) {
        return TokenKind::Kw(Kw::Public);
    }
    if version >= 3 && word_eq(word, "short", version) {
        return TokenKind::Kw(Kw::Short);
    }
    if word_eq(word, "else", version) {
        return TokenKind::Kw(Kw::Else);
    }
    if word_eq(word, "include", version) {
        return TokenKind::Kw(Kw::Include);
    }
    if version >= 3 && word_eq(word, "throws", version) {
        return TokenKind::Kw(Kw::Throws);
    }
    if version >= 3 && word_eq(word, "throw", version) {
        return TokenKind::Kw(Kw::Throw);
    }
    if version >= 3 && word_eq(word, "transient", version) {
        return TokenKind::Kw(Kw::Transient);
    }
    if version >= 3 && word_eq(word, "volatile", version) {
        return TokenKind::Kw(Kw::Volatile);
    }
    if version >= 3 && word_eq(word, "default", version) {
        return TokenKind::Kw(Kw::Default);
    }
    if version >= 3 && word_eq(word, "synchronized", version) {
        return TokenKind::Kw(Kw::Synchronized);
    }
    if version >= 3 && word_eq(word, "typeof", version) {
        return TokenKind::Kw(Kw::Typeof);
    }

    TokenKind::Ident
}

/// Keywords emitted as dedicated token types (Java `TokenType` from words).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kw {
    As,
    Var,
    Global,
    Return,
    Constructor,
    Final,
    For,
    If,
    While,
    Static,
    In,
    Abstract,
    Await,
    Break,
    Continue,
    Import,
    Export,
    Goto,
    Switch,
    Super,
    Class,
    Catch,
    Extends,
    True,
    False,
    Const,
    Char,
    Enum,
    Eval,
    Case,
    Float,
    Double,
    Byte,
    Do,
    Try,
    Void,
    With,
    Yield,
    Finally,
    Interface,
    Long,
    Let,
    Native,
    New,
    Package,
    This,
    Function,
    Implements,
    Int,
    Not,
    Null,
    Private,
    Protected,
    Public,
    Short,
    Else,
    Include,
    Throws,
    Throw,
    Transient,
    Volatile,
    Default,
    Synchronized,
    Typeof,
}

/// Word forms that Java maps to `TokenType.OPERATOR` (not `STRING`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WordOp {
    And,
    Or,
    Xor,
    Is,
    Instanceof,
}
