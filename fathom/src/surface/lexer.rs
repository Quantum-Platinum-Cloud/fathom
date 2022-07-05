use codespan_reporting::diagnostic::{Diagnostic, Label};
use logos::Logos;

use crate::source::{ByteRange, FileId};

#[derive(Clone, Debug, Logos)]
pub enum Token<'source> {
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Name(&'source str),
    #[regex(r"\?[a-zA-Z_][a-zA-Z0-9_]*", |lex| &lex.slice()[1..])]
    Hole(&'source str),
    #[regex(r#""([^"\\]|\\.)*""#, |lex| &lex.slice()[1..(lex.slice().len() - 1)])]
    StringLiteral(&'source str),
    #[regex(r"[+-]?[0-9][a-zA-Z0-9_]*")]
    NumberLiteral(&'source str),

    #[token("def")]
    KeywordDef,
    #[token("false")]
    KeywordFalse,
    #[token("fun")]
    KeywordFun,
    #[token("let")]
    KeywordLet,
    #[token("match")]
    KeywordMatch,
    #[token("overlap")]
    KeywordOverlap,
    #[token("Type")]
    KeywordType,
    #[token("true")]
    KeywordTrue,
    #[token("where")]
    KeywordWhere,

    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token("=")]
    Equals,
    #[token("=>")]
    EqualsGreater,
    #[token(".")]
    FullStop,
    #[token("->")]
    HyphenGreater,
    #[token("<-")]
    LessHyphen,
    #[token("-")]
    Minus,
    #[token("|")]
    Pipe,
    #[token("+")]
    Plus,
    #[token(";")]
    Semicolon,
    #[token("_")]
    Underscore,

    #[token("{")]
    OpenBrace,
    #[token("}")]
    CloseBrace,
    #[token("[")]
    OpenBracket,
    #[token("]")]
    CloseBracket,
    #[token("(")]
    OpenParen,
    #[token(")")]
    CloseParen,

    #[error]
    #[regex(r"\p{Whitespace}", logos::skip)]
    #[regex(r"//(.*)\n", logos::skip)]
    Error,
}

pub type Spanned<Tok, Loc> = (Loc, Tok, Loc);

#[derive(Clone, Debug)]
pub enum Error {
    UnexpectedCharacter { range: ByteRange },
}

impl Error {
    pub fn range(&self) -> ByteRange {
        match self {
            Error::UnexpectedCharacter { range } => *range,
        }
    }

    pub fn to_diagnostic(&self) -> Diagnostic<FileId> {
        match self {
            Error::UnexpectedCharacter { range } => Diagnostic::error()
                .with_message("unexpected character")
                .with_labels(vec![Label::primary(range.file_id(), *range)]),
        }
    }
}

pub fn tokens<'source>(
    file_id: FileId,
    source: &'source str,
) -> impl 'source + Iterator<Item = Result<Spanned<Token<'source>, usize>, Error>> {
    Token::lexer(source)
        .spanned()
        .map(move |(token, range)| match token {
            Token::Error => Err(Error::UnexpectedCharacter {
                range: ByteRange::new(file_id, range.start, range.end),
            }),
            token => Ok((range.start, token, range.end)),
        })
}

impl<'source> Token<'source> {
    pub fn description(&self) -> &'static str {
        match self {
            Token::Name(_) => "name",
            Token::Hole(_) => "hole",
            Token::StringLiteral(_) => "string literal",
            Token::NumberLiteral(_) => "number literal",
            Token::KeywordDef => "def",
            Token::KeywordFalse => "false",
            Token::KeywordFun => "fun",
            Token::KeywordLet => "let",
            Token::KeywordMatch => "match",
            Token::KeywordOverlap => "overlap",
            Token::KeywordTrue => "true",
            Token::KeywordType => "Type",
            Token::KeywordWhere => "where",
            Token::Colon => ":",
            Token::Comma => ",",
            Token::Equals => "=>",
            Token::EqualsGreater => "=>",
            Token::FullStop => ".",
            Token::HyphenGreater => "->",
            Token::LessHyphen => "<-",
            Token::Minus => "-",
            Token::Semicolon => ";",
            Token::Pipe => "|",
            Token::Plus => "+",
            Token::Underscore => "_",
            Token::OpenBrace => "{",
            Token::CloseBrace => "}",
            Token::OpenBracket => "[",
            Token::CloseBracket => "]",
            Token::OpenParen => "(",
            Token::CloseParen => ")",
            Token::Error => "error",
        }
    }
}
