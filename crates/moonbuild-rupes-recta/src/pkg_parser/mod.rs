mod syntax;
use logos::{Lexer, Logos};
use moonutil::package::Import;
use syntax::{Argument, Constant, Expr, ImportItem, ImportKind, MapElem, Statement};

use std::{cell::Cell, fmt::Display, ops::Range};

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\f]+")]
pub enum Token {
    #[token("[", with_span)]
    LBRACKET(Range<usize>),
    #[token("]", with_span)]
    RBRACKET(Range<usize>),
    #[token(",", with_span)]
    COMMA(Range<usize>),
    #[token(":", with_span)]
    COLON(Range<usize>),
    #[token("{", with_span)]
    LBRACE(Range<usize>),
    #[token("}", with_span)]
    RBRACE(Range<usize>),
    #[token("=", with_span)]
    EQUAL(Range<usize>),
    #[token("(", with_span)]
    LPAREN(Range<usize>),
    #[token(")", with_span)]
    RPAREN(Range<usize>),
    #[token(";", with_span)]
    SEMI(Range<usize>),
    #[token("true", with_span)]
    TRUE(Range<usize>),
    #[token("false", with_span)]
    FALSE(Range<usize>),
    #[regex(r#""([^"\\]|\\.)*""#, with_string)]
    STRING((Range<usize>, String)),
    #[regex(r"-?[0-9]+", with_int)]
    INT((Range<usize>, i32)),
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", with_lexeme)]
    LIDENT((Range<usize>, String)),
    #[token("as", with_span)]
    AS(Range<usize>),
    #[token("import", with_span)]
    IMPORT(Range<usize>),
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*", with_package_name)]
    PACKAGENAME((Range<usize>, String)),
    EOF(Range<usize>),
}

#[derive(Debug, PartialEq)]
enum TokenKind {
    LBRACKET,
    RBRACKET,
    COMMA,
    COLON,
    LBRACE,
    RBRACE,
    EQUAL,
    LPAREN,
    RPAREN,
    SEMI,
    TRUE,
    FALSE,
    STRING,
    INT,
    LIDENT,
    AS,
    IMPORT,
    PACKAGENAME,
    EOF,
}

impl Token {
    fn range(&self) -> &Range<usize> {
        match self {
            Token::LBRACKET(r)
            | Token::RBRACKET(r)
            | Token::COMMA(r)
            | Token::COLON(r)
            | Token::LBRACE(r)
            | Token::RBRACE(r)
            | Token::EQUAL(r)
            | Token::LPAREN(r)
            | Token::RPAREN(r)
            | Token::SEMI(r)
            | Token::TRUE(r)
            | Token::FALSE(r)
            | Token::AS(r)
            | Token::IMPORT(r)
            | Token::EOF(r)
            | Token::STRING((r, _))
            | Token::INT((r, _))
            | Token::PACKAGENAME((r, _))
            | Token::LIDENT((r, _)) => r,
        }
    }
    fn kind(&self) -> TokenKind {
        match self {
            Token::LBRACKET(_) => TokenKind::LBRACKET,
            Token::RBRACKET(_) => TokenKind::RBRACKET,
            Token::COMMA(_) => TokenKind::COMMA,
            Token::COLON(_) => TokenKind::COLON,
            Token::LBRACE(_) => TokenKind::LBRACE,
            Token::RBRACE(_) => TokenKind::RBRACE,
            Token::EQUAL(_) => TokenKind::EQUAL,
            Token::LPAREN(_) => TokenKind::LPAREN,
            Token::RPAREN(_) => TokenKind::RPAREN,
            Token::SEMI(_) => TokenKind::SEMI,
            Token::TRUE(_) => TokenKind::TRUE,
            Token::FALSE(_) => TokenKind::FALSE,
            Token::STRING(_) => TokenKind::STRING,
            Token::INT(_) => TokenKind::INT,
            Token::LIDENT(_) => TokenKind::LIDENT,
            Token::AS(_) => TokenKind::AS,
            Token::IMPORT(_) => TokenKind::IMPORT,
            Token::EOF(_) => TokenKind::EOF,
            Token::PACKAGENAME(_) => TokenKind::PACKAGENAME,
        }
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::LBRACKET(_) => write!(f, "["),
            Token::RBRACKET(_) => write!(f, "]"),
            Token::COMMA(_) => write!(f, ","),
            Token::COLON(_) => write!(f, ":"),
            Token::LBRACE(_) => write!(f, "{{"),
            Token::RBRACE(_) => write!(f, "}}"),
            Token::EQUAL(_) => write!(f, "="),
            Token::LPAREN(_) => write!(f, "("),
            Token::RPAREN(_) => write!(f, ")"),
            Token::SEMI(_) => write!(f, ";"),
            Token::TRUE(_) => write!(f, "true"),
            Token::FALSE(_) => write!(f, "false"),
            Token::STRING((_, s)) => write!(f, "\"{}\"", s),
            Token::INT((_, s)) => write!(f, "{}", s),
            Token::LIDENT((_, s)) => write!(f, "{}", s),
            Token::AS(_) => write!(f, "as"),
            Token::IMPORT(_) => write!(f, "import"),
            Token::PACKAGENAME((_, s)) => write!(f, "@{}", s),
            Token::EOF(_) => write!(f, "<EOF>"),
        }
    }
}

fn with_span<'a>(lex: &mut Lexer<'a, Token>) -> Range<usize> {
    lex.span()
}

fn with_lexeme<'a>(lex: &mut Lexer<'a, Token>) -> (Range<usize>, String) {
    let s = lex.slice();
    let span = lex.span();
    let lexme = s.to_string();
    (span, lexme)
}

fn with_int<'a>(lex: &mut Lexer<'a, Token>) -> (Range<usize>, i32) {
    let s = lex.slice();
    let span = lex.span();
    let i = s.parse::<i32>().unwrap(); // Safe because regex ensures valid integer
    (span, i)
}

fn with_string<'a>(lex: &mut Lexer<'a, Token>) -> (Range<usize>, String) {
    let s = lex.slice();
    let span = lex.span();
    let lexme = s[1..s.len() - 1].to_string();
    (span, lexme)
}

fn with_package_name<'a>(lex: &mut Lexer<'a, Token>) -> (Range<usize>, String) {
    let span = lex.span();
    let lexme = lex.slice()[1..].to_string();
    (span, lexme)
}

pub struct Parser {
    tokens: Vec<Token>,
    index: Cell<usize>,
}

#[derive(Debug)]
pub enum ParseError {
    UnexpectedToken(Token),
}

impl Parser {
    fn peek(&self) -> &Token {
        self.peek_nth(0)
    }

    fn peek_nth(&self, n: usize) -> &Token {
        if self.index.get() + n >= self.tokens.len() {
            return &self.tokens[self.index.get() - 1];
        }
        &self.tokens[self.index.get() + n]
    }

    fn skip(&self) {
        self.index.set(self.index.get() + 1)
    }

    fn parse_id(&self) -> Result<String, ParseError> {
        match self.peek() {
            Token::LIDENT((_, s)) => {
                self.skip();
                Ok(s.clone())
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_string(&self) -> Result<String, ParseError> {
        match self.peek() {
            Token::STRING((_, s)) => {
                self.skip();
                Ok(s.clone())
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_constant(&self) -> Result<Constant, ParseError> {
        match self.peek() {
            Token::TRUE(_) => {
                self.skip();
                Ok(Constant::Bool(true))
            }
            Token::FALSE(_) => {
                self.skip();
                Ok(Constant::Bool(false))
            }
            Token::STRING((_, s)) => {
                self.skip();
                Ok(Constant::String(s.clone()))
            }
            Token::INT((_, i)) => {
                self.skip();
                Ok(Constant::Int(*i))
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn surround_series<T, F>(
        &self,
        l: TokenKind,
        r: TokenKind,
        sep: TokenKind,
        f: F,
    ) -> Result<Vec<T>, ParseError>
    where
        F: Fn(&Parser) -> Result<T, ParseError>,
    {
        if !(self.peek().kind() == l) {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        }
        self.skip(); // skip l
        let mut elems = Vec::new();
        loop {
            let expr = f(self)?;
            elems.push(expr);
            let next = self.peek().kind();
            if next == r {
                break;
            } else if next == sep {
                self.skip();
                // handle trailing comma
                if self.peek().kind() == r {
                    break;
                }
            } else {
                return Err(ParseError::UnexpectedToken(self.peek().clone()));
            }
        }
        self.skip(); // skip r
        Ok(elems)
    }

    fn parse_array(&self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::LBRACKET(_) => {
                let elems = self.surround_series(
                    TokenKind::LBRACKET,
                    TokenKind::RBRACKET,
                    TokenKind::COMMA,
                    |s| s.parse_expr(),
                );
                Ok(Expr::Array(elems?))
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_map_elem(&self) -> Result<MapElem, ParseError> {
        let key = self.parse_string()?;
        let Token::COLON(_) = self.peek() else {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        };
        self.skip();
        let value = self.parse_expr()?;
        Ok(MapElem { key, value })
    }

    fn parse_map(&self) -> Result<Expr, ParseError> {
        let elems = self.surround_series(
            TokenKind::LBRACE,
            TokenKind::RBRACE,
            TokenKind::COMMA,
            |s| s.parse_map_elem(),
        )?;
        Ok(Expr::Map(elems))
    }

    fn parse_expr(&self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::LIDENT(_) => self.parse_id().map(Expr::Id),
            Token::LBRACKET(_) => self.parse_array(),
            Token::LBRACE(_) => self.parse_map(),
            Token::TRUE(_) | Token::FALSE(_) | Token::STRING(_) | Token::INT(_) => {
                self.parse_constant().map(Expr::Constant)
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_assign_statement(&self) -> Result<Statement, ParseError> {
        let var_name = self.parse_id()?;
        let Token::EQUAL(_) = self.peek() else {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        };
        self.skip();
        let expr = self.parse_expr()?;
        Ok(Statement::Assign(var_name, expr))
    }

    fn parse_apply_statement(&self) -> Result<Statement, ParseError> {
        let func_name = self.parse_id()?;
        let args = self.surround_series(
            TokenKind::LPAREN,
            TokenKind::RPAREN,
            TokenKind::COMMA,
            |s| {
                match (s.peek(), s.peek_nth(1)) {
                    (Token::LIDENT((_, label)), Token::EQUAL(_)) => {
                        let label_name = label.clone();
                        // skip label
                        s.skip();
                        let Token::EQUAL(_) = s.peek() else {
                            return Err(ParseError::UnexpectedToken(s.peek().clone()));
                        };
                        // skip '='
                        s.skip();
                        let expr = s.parse_expr()?;
                        Ok(Argument::Labeled(label_name, expr))
                    }
                    _ => s.parse_expr().map(Argument::Positional),
                }
            },
        )?;
        Ok(Statement::Apply(func_name, args))
    }

    fn parse_import_statement(&self) -> Result<Statement, ParseError> {
        self.skip(); // skip 'import'
        let import_kind = match self.peek() {
            Token::STRING((_, s)) => match s.as_str() {
                "test" => {
                    self.skip();
                    ImportKind::Test
                }
                "wbtest" => {
                    self.skip();
                    ImportKind::Wbtest
                }
                _ => {
                    return Err(ParseError::UnexpectedToken(self.peek().clone()));
                }
            },
            _ => ImportKind::Regular,
        };
        let import_items = self.surround_series(
            TokenKind::LBRACE,
            TokenKind::RBRACE,
            TokenKind::COMMA,
            |s| {
                let path = s.parse_string()?;
                let alias = match s.peek() {
                    Token::AS(_) => {
                        s.skip();
                        let Token::PACKAGENAME((_, alias)) = s.peek() else {
                            return Err(ParseError::UnexpectedToken(s.peek().clone()));
                        };
                        s.skip();
                        Some(alias.clone())
                    }
                    _ => None,
                };
                Ok(ImportItem { path, alias })
            },
        )?;
        Ok(Statement::Import(import_items, import_kind))
    }

    fn parse_statement(&self) -> Result<Statement, ParseError> {
        match self.peek() {
            Token::IMPORT(_) => self.parse_import_statement(),
            Token::LIDENT(_) => {
                if let Token::LPAREN(_) = self.peek_nth(1) {
                    self.parse_apply_statement()
                } else if let Token::EQUAL(_) = self.peek_nth(1) {
                    self.parse_assign_statement()
                } else {
                    Err(ParseError::UnexpectedToken(self.peek().clone()))
                }
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_statements(&self) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();
        while self.peek().kind() != TokenKind::EOF {
            let stmt = self.parse_statement()?;
            if self.peek().kind() == TokenKind::SEMI {
                self.skip();
            }
            statements.push(stmt);
        }
        Ok(statements)
    }

    pub fn parse(tokens: Vec<Token>) -> Result<Vec<Statement>, ParseError> {
        let state = Parser {
            tokens,
            index: Cell::new(0),
        };
        state.parse_statements()
    }
}

#[test]
fn lex_test() {
    let tokens = Token::lexer(
        r#"
import { 
  "path/to/pkg1", 
  "path/to/pkg2" as @alias, 
}

import "test" { 
  "path/to/pkg1", 
}

is_main=true

build(
  command="wasmer run xx $input $output",
  input="input.mbt",
  output="output.moonpkg",
)

warnings(
  off      = [fragile_match],
  on       = [all],
  as_error = [deprecated_syntax],
)

formatter(
  ignore=[
    "file1.mbt",
    "file2.mbt",
  ]
)

supported_backends({
  "file1.mbt": [js, wasm],
  "file2.mbt": [native],
  "file3.mbt": [js, native, wasm],
})
  "#,
    );
    let mut vec = tokens
        .filter_map(|x| match x {
            Ok(t) => Some(t),
            Err(_) => None,
        })
        .collect::<Vec<_>>();
    vec.push(Token::EOF(0..0));
    println!("{:?}", vec);
    println!("{:#?}", Parser::parse(vec).unwrap());
}
