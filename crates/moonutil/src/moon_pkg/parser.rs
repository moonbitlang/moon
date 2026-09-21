// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

#[cfg(test)]
use crate::moon_pkg::tokenize;
use crate::moon_pkg::{lexer, syntax::Dsl};
use crate::target::TargetBackend;

use super::lexer::{Token, TokenKind};
use anyhow::anyhow;
use serde_json_lenient::{Map, Number, Value, json};
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
    IntegerOutOfRange { literal: String, loc: lexer::Loc },
    LexingError(Range<usize>),
    InvalidCfg { message: String, loc: lexer::Loc },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidCfg { message, loc } => write!(
                f,
                "{message} at line {}, column {}",
                loc.start.line, loc.start.column
            ),
            ParseError::UnexpectedToken(token) => {
                let loc = token.range();
                write!(
                    f,
                    "unexpected token {} at line {}, column {}",
                    token, loc.start.line, loc.start.column
                )
            }
            ParseError::IntegerOutOfRange { literal, loc } => {
                write!(
                    f,
                    "integer literal `{literal}` is out of range at line {}, column {}",
                    loc.start.line, loc.start.column
                )
            }
            ParseError::LexingError(range) => {
                write!(
                    f,
                    "lexing error at byte range {}..{}",
                    range.start, range.end
                )
            }
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
    fn peek_nth(&self, n: usize) -> &Token {
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

    /// Parse a series of elements surrounded by `l` and `r`, separated by `sep`,
    /// the parsing function `f` is used to parse each element.
    ///
    /// Note: allows trailing separator.
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

    fn parse_integer(&self) -> Result<Value, ParseError> {
        let Token::INT((loc, literal)) = self.peek() else {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        };
        // Preserve the integer range for field-level type checking during conversion.
        let number = literal
            .parse::<i64>()
            .map(Number::from)
            .or_else(|_| literal.parse::<u64>().map(Number::from))
            .map_err(|_| ParseError::IntegerOutOfRange {
                literal: literal.clone(),
                loc: loc.clone(),
            })?;
        self.skip();
        Ok(Value::Number(number))
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
            Token::INT(_) => self.parse_integer(),
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

    fn parse_assign_statement(&self) -> Result<(String, Value), ParseError> {
        let key = self.parse_id()?;
        let Token::EQUAL(_) = self.peek() else {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        };
        self.skip();
        let value = self.parse_expr()?;
        Ok((key, value))
    }

    /// Evaluate a target-only cfg expression to its exact set of backends.
    fn parse_cfg_expr(&self) -> Result<Vec<TargetBackend>, ParseError> {
        let token = self.peek().clone();
        match &token {
            Token::TRUE(_) => {
                self.skip();
                Ok(TargetBackend::all().to_vec())
            }
            Token::FALSE(_) => {
                self.skip();
                Ok(Vec::new())
            }
            Token::LIDENT((_, name)) if name == "target" => {
                self.skip();
                if self.peek().kind() != TokenKind::EQUAL {
                    return Err(ParseError::UnexpectedToken(self.peek().clone()));
                }
                self.skip();
                let loc = self.peek().range().clone();
                let name = self.parse_string()?;
                let target =
                    TargetBackend::str_to_backend(&name).map_err(|err| ParseError::InvalidCfg {
                        message: err.to_string(),
                        loc,
                    })?;
                Ok(vec![target])
            }
            Token::LIDENT((loc, op)) if matches!(op.as_str(), "all" | "any" | "not") => {
                self.skip();
                let args = self.surround_series(
                    TokenKind::LPAREN,
                    TokenKind::RPAREN,
                    TokenKind::COMMA,
                    Self::parse_cfg_expr,
                )?;
                if op == "not" && args.len() != 1 {
                    return Err(ParseError::InvalidCfg {
                        message: "`not` requires exactly one cfg expression".to_owned(),
                        loc: loc.clone(),
                    });
                }
                Ok(TargetBackend::all()
                    .iter()
                    .copied()
                    .filter(|backend| match op.as_str() {
                        "all" => args.iter().all(|targets| targets.contains(backend)),
                        "any" => args.iter().any(|targets| targets.contains(backend)),
                        "not" => !args[0].contains(backend),
                        _ => unreachable!(),
                    })
                    .collect())
            }
            _ => Err(ParseError::UnexpectedToken(token)),
        }
    }

    fn parse_conditional_import(&self) -> Result<(String, Value), ParseError> {
        let mut targets = TargetBackend::all().to_vec();
        while self.peek().kind() == TokenKind::CFG {
            let loc = self.peek().range().clone();
            self.skip();
            let args = self.surround_series(
                TokenKind::LPAREN,
                TokenKind::RPAREN,
                TokenKind::COMMA,
                Self::parse_cfg_expr,
            )?;
            let [condition] = args.as_slice() else {
                return Err(ParseError::InvalidCfg {
                    message: "`#cfg` requires exactly one cfg expression".to_owned(),
                    loc,
                });
            };
            targets.retain(|backend| condition.contains(backend));
        }
        if self.peek().kind() != TokenKind::IMPORT {
            return Err(ParseError::UnexpectedToken(self.peek().clone()));
        }
        self.parse_import_statement(Some(&targets))
    }

    fn parse_import_statement(
        &self,
        targets: Option<&[TargetBackend]>,
    ) -> Result<(String, Value), ParseError> {
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
                match s.peek() {
                    Token::AS(_) => {
                        // deprecated syntax: "path/to/pkg" as @alias
                        s.skip();
                        let Token::PACKAGENAME((_, alias)) = s.peek() else {
                            return Err(ParseError::UnexpectedToken(s.peek().clone()));
                        };
                        let alias = alias.clone();
                        s.skip();
                        Ok(s.parse_import_object(path.clone(), Some(alias), targets))
                    }
                    Token::PACKAGENAME((_, alias)) => {
                        let alias = alias.clone();
                        s.skip();
                        Ok(s.parse_import_object(path.clone(), Some(alias), targets))
                    }
                    Token::STAR(_) => {
                        s.skip();
                        Ok(s.import_object(path.clone(), None, true, targets))
                    }
                    _ => Ok(s.import_object(path.clone(), None, false, targets)),
                }
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

    fn parse_import_object(
        &self,
        path: String,
        alias: Option<String>,
        targets: Option<&[TargetBackend]>,
    ) -> Value {
        let import_all = if matches!(self.peek(), Token::STAR(_)) {
            self.skip();
            true
        } else {
            false
        };
        self.import_object(path, alias, import_all, targets)
    }

    fn import_object(
        &self,
        path: String,
        alias: Option<String>,
        import_all: bool,
        targets: Option<&[TargetBackend]>,
    ) -> Value {
        if alias.is_none() && !import_all && targets.is_none() {
            return json!(path);
        }
        let mut object = Map::new();
        object.insert("path".to_string(), json!(path));
        if let Some(alias) = alias {
            object.insert("alias".to_string(), json!(alias));
        }
        if import_all {
            object.insert("import-all".to_string(), json!(true));
        }
        if let Some(targets) = targets {
            object.insert("targets".to_string(), json!(targets));
        }
        Value::Object(object)
    }

    fn parse_statement(&self) -> Result<(String, Value), ParseError> {
        match self.peek() {
            Token::IMPORT(_) => self.parse_import_statement(None),
            Token::CFG(_) => self.parse_conditional_import(),
            Token::LIDENT(_) => {
                if let Token::EQUAL(_) = self.peek_nth(1) {
                    self.parse_assign_statement()
                } else if let Token::LPAREN(_) = self.peek_nth(1) {
                    self.parse_apply_statement()
                } else {
                    Err(ParseError::UnexpectedToken(self.peek().clone()))
                }
            }
            other => Err(ParseError::UnexpectedToken(other.clone())),
        }
    }

    fn parse_statements(&self) -> Result<Vec<(String, Value)>, ParseError> {
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

    fn parse(tokens: Vec<Token>) -> Result<Dsl, ParseError> {
        let state = Parser {
            tokens,
            index: Cell::new(0),
        };
        let entries = state.parse_statements()?;
        Ok(Dsl { entries })
    }
}

/// Parse MoonPkg DSL input into ordered entries with JSON-shaped values.
/// Manifest conversion validates these entries and decodes their fields.
pub fn parse(input: &str) -> anyhow::Result<Dsl> {
    let tokens = lexer::tokenize(input)?;
    Parser::parse(tokens).map_err(|e| anyhow!("Parsing error: {e}"))
}

#[test]
fn parse_conditional_import_blocks() {
    for (prefix, suffix, key) in [
        ("", "", "import"),
        ("", "for \"test\"", "test-import"),
        ("", "for \"wbtest\"", "wbtest-import"),
        ("\"test\"", "", "test-import"),
        ("\"wbtest\"", "", "wbtest-import"),
    ] {
        let dsl = parse(&format!(
            r#"
import {{ "example/common" }}
#cfg(target = "native")
import {prefix} {{
  "example/default",
  "example/renamed" @renamed,
  "example/legacy" as @legacy,
  "example/all" *,
}} {suffix}
import {{ "example/after" }}
"#,
        ))
        .unwrap();
        assert_eq!(
            dsl.entries,
            vec![
                ("import".to_owned(), json!(["example/common"])),
                (
                    key.to_owned(),
                    json!([
                        {"path": "example/default", "targets": ["Native"]},
                        {"path": "example/renamed", "alias": "renamed", "targets": ["Native"]},
                        {"path": "example/legacy", "alias": "legacy", "targets": ["Native"]},
                        {"path": "example/all", "import-all": true, "targets": ["Native"]},
                    ]),
                ),
                ("import".to_owned(), json!(["example/after"])),
            ],
        );
    }
}

#[test]
fn parse_import_cfg_expressions() {
    let all = &["Wasm", "WasmGC", "Js", "Native", "LLVM"][..];
    for (condition, targets) in [
        ("true", all),
        ("false", &[]),
        ("all()", all),
        ("any()", &[]),
        ("not(false)", all),
        ("not(true)", &[]),
        (r#"target = "wasm""#, &["Wasm"]),
        (r#"target = "wasm-gc""#, &["WasmGC"]),
        (r#"target = "js""#, &["Js"]),
        (r#"target = "native""#, &["Native"]),
        (r#"target = "llvm""#, &["LLVM"]),
        (
            r#"any(target = "native", target = "js",)"#,
            &["Js", "Native"],
        ),
        (
            r#"all(any(target = "native", target = "js"), not(target = "js"), true)"#,
            &["Native"],
        ),
        (r#"all(target = "native", target = "js")"#, &[]),
    ] {
        let source = format!("#cfg({condition},) import {{ \"example/dep\" }};");
        let dsl = parse(&source).unwrap();
        assert_eq!(
            dsl.entries,
            vec![(
                "import".to_owned(),
                json!([{"path": "example/dep", "targets": targets}]),
            )],
            "{source}",
        );
    }
}

#[test]
fn parse_stacked_import_cfg() {
    let dsl = parse(
        r#"
#cfg(any(target = "native", target = "js"))
// Both attributes apply to the following block.
#cfg(not(target = "js"))
import { "example/dep" }
"#,
    )
    .unwrap();
    assert_eq!(
        dsl.entries,
        vec![(
            "import".to_owned(),
            json!([{"path": "example/dep", "targets": ["Native"]}]),
        )],
    );
}

#[test]
fn reject_invalid_import_cfg() {
    for source in [
        r#"#cfg(target = "unknown") import { "a/b" }"#,
        r#"#cfg(target = native) import { "a/b" }"#,
        r#"#cfg(target: "native") import { "a/b" }"#,
        r#"#cfg(os = "linux") import { "a/b" }"#,
        r#"#cfg(not()) import { "a/b" }"#,
        r#"#cfg(not(true, false)) import { "a/b" }"#,
        r#"#cfg() import { "a/b" }"#,
        r#"#cfg(true, false) import { "a/b" }"#,
        r#"#cfg(true) options("is-main": true)"#,
        r#"#cfg(true)"#,
        r#"import { #cfg(true) "a/b" }"#,
    ] {
        let error = parse(source).expect_err(source).to_string();
        assert!(error.contains("line 1, column"), "{source}: {error}");
    }
}

#[test]
fn parse_test() {
    let source = r#"
import {
  "path/to/pkg1",
  "path/to/pkg2" as @alias,
  "path/to/pkg3" @alias3,
}

import {
  "path/to/pkg1",
} for "test"

warnings = "-fragile_match+all@deprecated_syntax"

options(
  "is_main": true,
  "pre-build": [
    {
      "command": "wasmer run xx $input $output",
      "input": "input.mbt",
      "output": "output.moonpkg",
    }
  ],
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
        Dsl {
            entries: [
                (
                    "import",
                    Array [
                        String("path/to/pkg1"),
                        Object {
                            "path": String("path/to/pkg2"),
                            "alias": String("alias"),
                        },
                        Object {
                            "path": String("path/to/pkg3"),
                            "alias": String("alias3"),
                        },
                    ],
                ),
                (
                    "test-import",
                    Array [
                        String("path/to/pkg1"),
                    ],
                ),
                (
                    "warnings",
                    String("-fragile_match+all@deprecated_syntax"),
                ),
                (
                    "options",
                    Object {
                        "is_main": Bool(true),
                        "pre-build": Array [
                            Object {
                                "command": String("wasmer run xx $input $output"),
                                "input": String("input.mbt"),
                                "output": String("output.moonpkg"),
                            },
                        ],
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
                ),
                (
                    "f",
                    Object {
                        "label1": Array [],
                        "label2": Object {},
                    },
                ),
            ],
        }
    "#]]
    .assert_debug_eq(&ast);
}

#[test]
fn parse_import_alias_with_dash() {
    let source = r#"
import {
  "path/to/pkg" @pkg-A,
}
    "#;

    let tokens = tokenize(source).unwrap();
    let ast = Parser::parse(tokens).unwrap();

    assert_eq!(ast.entries.len(), 1);
    assert_eq!(ast.entries[0].0, "import");
    assert_eq!(
        ast.entries[0].1,
        json!([
            {
                "path": "path/to/pkg",
                "alias": "pkg-A"
            }
        ])
    );
}

#[test]
fn parse_import_all_alias() {
    let source = r#"
import {
  "path/to/pkg" *,
}
    "#;

    let tokens = tokenize(source).unwrap();
    let ast = Parser::parse(tokens).unwrap();

    assert_eq!(ast.entries.len(), 1);
    assert_eq!(ast.entries[0].0, "import");
    assert_eq!(
        ast.entries[0].1,
        json!([
            {
                "path": "path/to/pkg",
                "import-all": true
            }
        ])
    );
}

#[test]
fn parse_import_all_with_named_alias() {
    let source = r#"
import {
  "path/to/pkg" @pkg *,
}
    "#;

    let tokens = tokenize(source).unwrap();
    let ast = Parser::parse(tokens).unwrap();

    assert_eq!(ast.entries.len(), 1);
    assert_eq!(ast.entries[0].0, "import");
    assert_eq!(
        ast.entries[0].1,
        json!([
            {
                "path": "path/to/pkg",
                "alias": "pkg",
                "import-all": true
            }
        ])
    );
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
        Dsl {
            entries: [
                (
                    "test-import",
                    Array [
                        String("path/to/pkg1"),
                    ],
                ),
                (
                    "wbtest-import",
                    Array [
                        String("path/to/pkg2"),
                    ],
                ),
            ],
        }
    "#]]
    .assert_debug_eq(&ast);
}

#[test]
fn parse_reports_integer_overflow() {
    let literal = format!("1{}", "0".repeat(400));
    let error = parse(&format!("options(value: {literal})")).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("Parsing error: integer literal `{literal}` is out of range at line 1, column 16")
    );
}
