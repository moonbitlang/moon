// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use crate::moon_pkg::lexer;
#[cfg(test)]
use crate::moon_pkg::tokenize;

use super::lexer::{Loc, Token, TokenKind};
use anyhow::anyhow;
use serde_json_lenient::{Map, Value, json};
use std::{cell::Cell, fmt, ops::Range};

/// Parser for MoonPkg DSL
pub struct Parser {
    /// The whole token stream, including EOF
    tokens: Vec<Token>,
    /// Index of the next unconsumed token
    index: Cell<usize>,
}

#[derive(Debug)]
pub enum ParseError {
    UnexpectedToken(Token),
    LexingError(Range<usize>),
    UnexpectedTestBlock { loc: Loc },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnexpectedToken(token) => {
                let loc = token.range();
                write!(
                    f,
                    "unexpected token {} at line {}, column {}",
                    token, loc.start.line, loc.start.column
                )
            }
            ParseError::LexingError(range) => {
                write!(
                    f,
                    "lexing error at byte range {}..{}",
                    range.start, range.end
                )
            }
            ParseError::UnexpectedTestBlock { loc } => write!(
                f,
                "unexpected test block at line {}, column {}; moon.pkg does not support test declarations",
                loc.start.line, loc.start.column
            ),
        }
    }
}

impl Parser {
    /// Peek next unconsumed token
    pub fn peek(&self) -> &Token {
        self.peek_nth(0)
    }

    /// Peek the n-th unconsumed token
    ///
    /// If n == 0, same as `peek()`
    /// If n == 1, peek the token next to `peek()`
    /// If out of bounds, return the last token `EOF`
    pub fn peek_nth(&self, n: usize) -> &Token {
        if self.index.get() + n >= self.tokens.len() {
            return &self.tokens[self.index.get() - 1];
        }
        &self.tokens[self.index.get() + n]
    }

    /// Consume the next unconsumed token
    pub fn skip(&self) {
        self.index.set(self.index.get() + 1)
    }

    /// Parse an identifier token
    pub fn parse_id(&self) -> Result<String, ParseError> {
        match self.peek() {
            Token::LIDENT((_, s)) => {
                self.skip();
                Ok(s.clone())
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    pub fn parse_string(&self) -> Result<String, ParseError> {
        match self.peek() {
            Token::STRING((_, s)) => {
                self.skip();
                Ok(s.clone())
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    /// Parse a series of elements surrounded by `l` and `r`, separated by `sep`,
    /// the parsing function `f` is used to parse each element.
    ///
    /// Note: allows trailing separator.
    pub fn surround_series<T, F>(
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
        if !(self.peek().kind() == r) {
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
        }
        self.skip(); // skip r
        Ok(elems)
    }

    fn parse_array(&self) -> Result<Value, ParseError> {
        match self.peek() {
            Token::LBRACKET(_) => {
                let elems = self.surround_series(
                    TokenKind::LBRACKET,
                    TokenKind::RBRACKET,
                    TokenKind::COMMA,
                    |s| s.parse_expr(),
                );
                Ok(json!(elems?))
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_map_elem(&self) -> Result<(String, Value), ParseError> {
        let key = self.parse_string()?;
        let Token::COLON(_) = self.peek() else {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        };
        self.skip();
        let value = self.parse_expr()?;
        Ok((key, value))
    }

    fn parse_map(&self) -> Result<Value, ParseError> {
        let elems = self.surround_series(
            TokenKind::LBRACE,
            TokenKind::RBRACE,
            TokenKind::COMMA,
            |s| s.parse_map_elem(),
        )?;
        Ok(Value::Object(Map::from_iter(elems)))
    }

    fn parse_expr(&self) -> Result<Value, ParseError> {
        match self.peek() {
            Token::LBRACKET(_) => self.parse_array(),
            Token::LBRACE(_) => self.parse_map(),
            Token::TRUE(_) => {
                self.skip();
                Ok(json!(true))
            }
            Token::FALSE(_) => {
                self.skip();
                Ok(json!(false))
            }
            Token::STRING((_, s)) => {
                self.skip();
                Ok(json!(s))
            }
            Token::INT((_, i)) => {
                self.skip();
                Ok(json!(i))
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_apply(&self) -> Result<(String, Value), ParseError> {
        let func_name = self.parse_id()?;
        let args = self.surround_series(
            TokenKind::LPAREN,
            TokenKind::RPAREN,
            TokenKind::COMMA,
            |s| {
                match (s.peek(), s.peek_nth(1)) {
                    (Token::LIDENT((_, key)) | Token::STRING((_, key)), Token::COLON(_)) => {
                        // skip label
                        s.skip();
                        let Token::COLON(_) = s.peek() else {
                            return Err(ParseError::UnexpectedToken(s.peek().clone()));
                        };
                        // skip ':'
                        s.skip();
                        let expr = s.parse_expr()?;
                        Ok((key.clone(), expr))
                    }
                    (other, _) => Err(ParseError::UnexpectedToken(other.clone())),
                }
            },
        )?;
        Ok((func_name, Value::Object(Map::from_iter(args))))
    }

    /// Leave this wrapper for clarity
    fn parse_apply_statement(&self) -> Result<(String, Value), ParseError> {
        self.parse_apply()
    }

    fn parse_import_statement(&self) -> Result<(String, Value), ParseError> {
        self.skip(); // skip 'import'
        let legacy_kind = match self.peek() {
            // Legacy syntax: import "test" { ... } / import "wbtest" { ... }.
            Token::STRING((_, s)) if s == "test" => {
                self.skip();
                Some("test-import")
            }
            Token::STRING((_, s)) if s == "wbtest" => {
                self.skip();
                Some("wbtest-import")
            }
            Token::STRING((_, _)) => {
                return Err(ParseError::UnexpectedToken(self.peek().clone()));
            }
            _ => None,
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
                Ok(match alias {
                    None => json!(path),
                    Some(s) => json!({"path": path, "alias": s}),
                })
            },
        )?;
        let import_kind = if let Some(kind) = legacy_kind {
            kind
        } else if let Token::FOR(_) = self.peek() {
            self.skip();
            let kind = match self.peek() {
                Token::STRING((_, s)) if s == "test" => "test-import",
                Token::STRING((_, s)) if s == "wbtest" => "wbtest-import",
                _ => {
                    return Err(ParseError::UnexpectedToken(self.peek().clone()));
                }
            };
            self.skip();
            kind
        } else {
            "import"
        };
        Ok((String::from(import_kind), Value::Array(import_items)))
    }

    fn parse_statement(&self) -> Result<(String, Value), ParseError> {
        if let Token::LIDENT((loc, ident)) = self.peek()
            && ident == "test"
            && matches!(self.peek_nth(1), Token::STRING(_))
        {
            return Err(ParseError::UnexpectedTestBlock { loc: loc.clone() });
        }
        match self.peek() {
            Token::IMPORT(_) => self.parse_import_statement(),
            Token::LIDENT(_) => {
                if let Token::LPAREN(_) = self.peek_nth(1) {
                    self.parse_apply_statement()
                } else {
                    Err(ParseError::UnexpectedToken(self.peek().clone()))
                }
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_statements(&self) -> Result<Value, ParseError> {
        let mut statements = Vec::new();
        while self.peek().kind() != TokenKind::EOF {
            let stmt = self.parse_statement()?;
            if self.peek().kind() == TokenKind::SEMI {
                self.skip();
            }
            statements.push(stmt);
        }
        Ok(Value::Object(Map::from_iter(statements)))
    }

    fn parse(tokens: Vec<Token>) -> Result<Value, ParseError> {
        let state = Parser {
            tokens,
            index: Cell::new(0),
        };
        state.parse_statements()
    }
}

/// Parse MoonPkg DSL input string into serde_json_lenient::Value
pub fn parse(input: &str) -> anyhow::Result<Value> {
    let tokens = lexer::tokenize(input)?;
    Parser::parse(tokens).map_err(|e| anyhow!("Parsing error: {:?}", e))
}

#[test]
fn parse_test() {
    let source = r#"
import {
  "path/to/pkg1",
  "path/to/pkg2" as @alias,
}

import {
  "path/to/pkg1",
} for "test"

options(
  "is_main": true,
  "pre-build": [
    {
      "command": "wasmer run xx $input $output",
      "input": "input.mbt",
      "output": "output.moonpkg",
    }
  ],
  warnings: "-fragile_match+all@deprecated_syntax",
  formatter: {
    "ignore": [
      "file1.mbt",
      "file2.mbt",
    ],
  },
  "supported-backends": {
    "file1.mbt": ["or", "js", ["and", "wasm", "release"]],
    "file2.mbt": ["native"],
    "file3.mbt": ["native"],
  },
)

f(
  label1: [],
  label2: {},
) 

    "#;

    let tokens = tokenize(source).unwrap();
    let ast = Parser::parse(tokens).unwrap();
    expect_test::expect![[r#"
        Object {
            "import": Array [
                String("path/to/pkg1"),
                Object {
                    "path": String("path/to/pkg2"),
                    "alias": String("alias"),
                },
            ],
            "test-import": Array [
                String("path/to/pkg1"),
            ],
            "options": Object {
                "is_main": Bool(true),
                "pre-build": Array [
                    Object {
                        "command": String("wasmer run xx $input $output"),
                        "input": String("input.mbt"),
                        "output": String("output.moonpkg"),
                    },
                ],
                "warnings": String("-fragile_match+all@deprecated_syntax"),
                "formatter": Object {
                    "ignore": Array [
                        String("file1.mbt"),
                        String("file2.mbt"),
                    ],
                },
                "supported-backends": Object {
                    "file1.mbt": Array [
                        String("or"),
                        String("js"),
                        Array [
                            String("and"),
                            String("wasm"),
                            String("release"),
                        ],
                    ],
                    "file2.mbt": Array [
                        String("native"),
                    ],
                    "file3.mbt": Array [
                        String("native"),
                    ],
                },
            },
            "f": Object {
                "label1": Array [],
                "label2": Object {},
            },
        }
    "#]]
    .assert_debug_eq(&ast);
}

#[test]
fn parse_test_block_error() {
    let source = r#"test "abc" { }"#;
    let tokens = tokenize(source).unwrap();
    let err = Parser::parse(tokens).unwrap_err();
    expect_test::expect![[
        r#"unexpected test block at line 1, column 1; moon.pkg does not support test declarations"#
    ]]
    .assert_eq(&err.to_string());
}

#[test]
fn parse_legacy_import_syntax() {
    let source = r#"
import "test" {
  "path/to/pkg1",
}

import "wbtest" {
  "path/to/pkg2",
}
"#;
    let tokens = tokenize(source).unwrap();
    let ast = Parser::parse(tokens).unwrap();
    expect_test::expect![[r#"
        Object {
            "test-import": Array [
                String("path/to/pkg1"),
            ],
            "wbtest-import": Array [
                String("path/to/pkg2"),
            ],
        }
    "#]]
    .assert_debug_eq(&ast);
}
