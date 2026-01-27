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

use anyhow::anyhow;
use logos::{Lexer, Logos, Skip};
use std::{fmt::Display, ops::Range};

/// Position in the source code
#[derive(Debug, PartialEq, Clone)]
pub struct Pos {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
}

/// Location range in the source code, from start (inclusive) to end (exclusive)
pub type Loc = Range<Pos>;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(extras = (usize, usize))]
#[logos(skip(r"(\n|\r\n)", newline_callback))]
#[logos(skip(r"//[^\n\r]*"))] // Skip single-line comments
#[logos(skip(r"[ \t\f]+"))]
pub enum Token {
    #[token("[", with_span)]
    LBRACKET(Loc),
    #[token("]", with_span)]
    RBRACKET(Loc),
    #[token(",", with_span)]
    COMMA(Loc),
    #[token(":", with_span)]
    COLON(Loc),
    #[token("{", with_span)]
    LBRACE(Loc),
    #[token("}", with_span)]
    RBRACE(Loc),
    #[token("=", with_span)]
    EQUAL(Loc),
    #[token("(", with_span)]
    LPAREN(Loc),
    #[token(")", with_span)]
    RPAREN(Loc),
    #[token(";", with_span)]
    SEMI(Loc),
    #[token("true", with_span)]
    TRUE(Loc),
    #[token("false", with_span)]
    FALSE(Loc),
    #[token("for", with_span)]
    FOR(Loc),
    #[regex(r#""([^"\\]|\\.)*""#, with_string)]
    STRING((Loc, String)),
    #[regex(r"-?[0-9]+", with_int)]
    INT((Loc, i32)),
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", with_lexeme)]
    LIDENT((Loc, String)),
    #[token("as", with_span)]
    AS(Loc),
    #[token("import", with_span)]
    IMPORT(Loc),
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_/]*", with_package_name)]
    PACKAGENAME((Loc, String)),
    EOF(Loc),
}

fn newline_callback(lex: &mut Lexer<Token>) -> Skip {
    lex.extras.0 += 1;
    lex.extras.1 = lex.span().end;
    Skip
}

fn get_loc<'a>(lex: &mut Lexer<'a, Token>) -> Loc {
    let span = lex.span();
    let start = Pos {
        line: lex.extras.0,
        column: span.start - lex.extras.1 + 1,
    };
    let end = Pos {
        line: lex.extras.0,
        column: span.end - lex.extras.1 + 1,
    };
    start..end
}

fn with_span<'a>(lex: &mut Lexer<'a, Token>) -> Loc {
    get_loc(lex)
}

fn with_lexeme<'a>(lex: &mut Lexer<'a, Token>) -> (Loc, String) {
    let s = lex.slice();
    let loc = get_loc(lex);
    let lexme = s.to_string();
    (loc, lexme)
}

fn with_int<'a>(lex: &mut Lexer<'a, Token>) -> (Loc, i32) {
    let s = lex.slice();
    let loc = get_loc(lex);
    let i = s.parse::<i32>().unwrap(); // Safe because regex ensures valid integer
    (loc, i)
}

fn with_string<'a>(lex: &mut Lexer<'a, Token>) -> Result<(Loc, String), ()> {
    let s = lex.slice();
    let loc = get_loc(lex);
    let inner = &s[1..s.len() - 1];
    let lexme = unescape_string(inner).ok_or(())?;
    Ok((loc, lexme))
}

fn unescape_string(s: &str) -> Option<String> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                'f' => result.push('\x0C'),
                'b' => result.push('\x08'),
                '\\' => result.push('\\'),
                '"' => result.push('"'),
                '/' => result.push('/'),
                'u' => {
                    // Parse \uXXXX unicode escape
                    let mut hex = String::with_capacity(4);
                    for _ in 0..4 {
                        hex.push(chars.next()?);
                    }
                    let code_point = u32::from_str_radix(&hex, 16).ok()?;
                    let ch = char::from_u32(code_point)?;
                    result.push(ch);
                }
                _ => return None, // Invalid escape sequence
            }
        } else {
            result.push(c);
        }
    }

    Some(result)
}

fn with_package_name<'a>(lex: &mut Lexer<'a, Token>) -> (Loc, String) {
    let loc = get_loc(lex);
    let lexme = lex.slice()[1..].to_string();
    (loc, lexme)
}

#[derive(Debug, PartialEq)]
pub enum TokenKind {
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
    FOR,
    STRING,
    INT,
    LIDENT,
    AS,
    IMPORT,
    PACKAGENAME,
    EOF,
}

impl Token {
    pub fn range(&self) -> &Loc {
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
            | Token::FOR(r)
            | Token::AS(r)
            | Token::IMPORT(r)
            | Token::EOF(r)
            | Token::STRING((r, _))
            | Token::INT((r, _))
            | Token::PACKAGENAME((r, _))
            | Token::LIDENT((r, _)) => r,
        }
    }
    pub fn kind(&self) -> TokenKind {
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
            Token::FOR(_) => TokenKind::FOR,
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
            Token::FOR(_) => write!(f, "for"),
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

pub fn tokenize(input: &str) -> anyhow::Result<Vec<Token>> {
    let mut lexer = Token::lexer(input);
    // Current line number and offset of current line start
    lexer.extras = (1, 0);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        match token {
            Ok(t) => tokens.push(t),
            Err(_) => return Err(anyhow!("Lexing error at {:?}", lexer.span())),
        }
    }
    let pos = Pos {
        line: lexer.extras.0,
        column: lexer.span().end - lexer.extras.1 + 1,
    };
    tokens.push(Token::EOF(pos.clone()..pos));
    Ok(tokens)
}

#[test]
fn tokenize_test() {
    let input = r#"import {
  "path/to/pkg1" as @path/alias,
  "path/to/pkg2" as @alias,
}

import {
  "path/to/pkg1",
} for "test"

warnings = "-fragile_match+all@deprecated_syntax"

options(
  "is_main": true,
  formatter: {
    "ignore": [
      "file1.mbt",
    ],
  },
)

  "#;
    let tokens = tokenize(input);
    expect_test::expect![[r#"
        Ok(
            [
                IMPORT(
                    Pos {
                        line: 1,
                        column: 1,
                    }..Pos {
                        line: 1,
                        column: 7,
                    },
                ),
                LBRACE(
                    Pos {
                        line: 1,
                        column: 8,
                    }..Pos {
                        line: 1,
                        column: 9,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 2,
                            column: 3,
                        }..Pos {
                            line: 2,
                            column: 17,
                        },
                        "path/to/pkg1",
                    ),
                ),
                AS(
                    Pos {
                        line: 2,
                        column: 18,
                    }..Pos {
                        line: 2,
                        column: 20,
                    },
                ),
                PACKAGENAME(
                    (
                        Pos {
                            line: 2,
                            column: 21,
                        }..Pos {
                            line: 2,
                            column: 32,
                        },
                        "path/alias",
                    ),
                ),
                COMMA(
                    Pos {
                        line: 2,
                        column: 32,
                    }..Pos {
                        line: 2,
                        column: 33,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 3,
                            column: 3,
                        }..Pos {
                            line: 3,
                            column: 17,
                        },
                        "path/to/pkg2",
                    ),
                ),
                AS(
                    Pos {
                        line: 3,
                        column: 18,
                    }..Pos {
                        line: 3,
                        column: 20,
                    },
                ),
                PACKAGENAME(
                    (
                        Pos {
                            line: 3,
                            column: 21,
                        }..Pos {
                            line: 3,
                            column: 27,
                        },
                        "alias",
                    ),
                ),
                COMMA(
                    Pos {
                        line: 3,
                        column: 27,
                    }..Pos {
                        line: 3,
                        column: 28,
                    },
                ),
                RBRACE(
                    Pos {
                        line: 4,
                        column: 1,
                    }..Pos {
                        line: 4,
                        column: 2,
                    },
                ),
                IMPORT(
                    Pos {
                        line: 6,
                        column: 1,
                    }..Pos {
                        line: 6,
                        column: 7,
                    },
                ),
                LBRACE(
                    Pos {
                        line: 6,
                        column: 8,
                    }..Pos {
                        line: 6,
                        column: 9,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 7,
                            column: 3,
                        }..Pos {
                            line: 7,
                            column: 17,
                        },
                        "path/to/pkg1",
                    ),
                ),
                COMMA(
                    Pos {
                        line: 7,
                        column: 17,
                    }..Pos {
                        line: 7,
                        column: 18,
                    },
                ),
                RBRACE(
                    Pos {
                        line: 8,
                        column: 1,
                    }..Pos {
                        line: 8,
                        column: 2,
                    },
                ),
                FOR(
                    Pos {
                        line: 8,
                        column: 3,
                    }..Pos {
                        line: 8,
                        column: 6,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 8,
                            column: 7,
                        }..Pos {
                            line: 8,
                            column: 13,
                        },
                        "test",
                    ),
                ),
                LIDENT(
                    (
                        Pos {
                            line: 10,
                            column: 1,
                        }..Pos {
                            line: 10,
                            column: 9,
                        },
                        "warnings",
                    ),
                ),
                EQUAL(
                    Pos {
                        line: 10,
                        column: 10,
                    }..Pos {
                        line: 10,
                        column: 11,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 10,
                            column: 12,
                        }..Pos {
                            line: 10,
                            column: 50,
                        },
                        "-fragile_match+all@deprecated_syntax",
                    ),
                ),
                LIDENT(
                    (
                        Pos {
                            line: 12,
                            column: 1,
                        }..Pos {
                            line: 12,
                            column: 8,
                        },
                        "options",
                    ),
                ),
                LPAREN(
                    Pos {
                        line: 12,
                        column: 8,
                    }..Pos {
                        line: 12,
                        column: 9,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 13,
                            column: 3,
                        }..Pos {
                            line: 13,
                            column: 12,
                        },
                        "is_main",
                    ),
                ),
                COLON(
                    Pos {
                        line: 13,
                        column: 12,
                    }..Pos {
                        line: 13,
                        column: 13,
                    },
                ),
                TRUE(
                    Pos {
                        line: 13,
                        column: 14,
                    }..Pos {
                        line: 13,
                        column: 18,
                    },
                ),
                COMMA(
                    Pos {
                        line: 13,
                        column: 18,
                    }..Pos {
                        line: 13,
                        column: 19,
                    },
                ),
                LIDENT(
                    (
                        Pos {
                            line: 14,
                            column: 3,
                        }..Pos {
                            line: 14,
                            column: 12,
                        },
                        "formatter",
                    ),
                ),
                COLON(
                    Pos {
                        line: 14,
                        column: 12,
                    }..Pos {
                        line: 14,
                        column: 13,
                    },
                ),
                LBRACE(
                    Pos {
                        line: 14,
                        column: 14,
                    }..Pos {
                        line: 14,
                        column: 15,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 15,
                            column: 5,
                        }..Pos {
                            line: 15,
                            column: 13,
                        },
                        "ignore",
                    ),
                ),
                COLON(
                    Pos {
                        line: 15,
                        column: 13,
                    }..Pos {
                        line: 15,
                        column: 14,
                    },
                ),
                LBRACKET(
                    Pos {
                        line: 15,
                        column: 15,
                    }..Pos {
                        line: 15,
                        column: 16,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 16,
                            column: 7,
                        }..Pos {
                            line: 16,
                            column: 18,
                        },
                        "file1.mbt",
                    ),
                ),
                COMMA(
                    Pos {
                        line: 16,
                        column: 18,
                    }..Pos {
                        line: 16,
                        column: 19,
                    },
                ),
                RBRACKET(
                    Pos {
                        line: 17,
                        column: 5,
                    }..Pos {
                        line: 17,
                        column: 6,
                    },
                ),
                COMMA(
                    Pos {
                        line: 17,
                        column: 6,
                    }..Pos {
                        line: 17,
                        column: 7,
                    },
                ),
                RBRACE(
                    Pos {
                        line: 18,
                        column: 3,
                    }..Pos {
                        line: 18,
                        column: 4,
                    },
                ),
                COMMA(
                    Pos {
                        line: 18,
                        column: 4,
                    }..Pos {
                        line: 18,
                        column: 5,
                    },
                ),
                RPAREN(
                    Pos {
                        line: 19,
                        column: 1,
                    }..Pos {
                        line: 19,
                        column: 2,
                    },
                ),
                EOF(
                    Pos {
                        line: 21,
                        column: 3,
                    }..Pos {
                        line: 21,
                        column: 3,
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&tokens);
}

#[test]
fn test_comment_lexing() {
    // Single-line comments should be skipped
    let input = r#"
    // this is a comment
    import // another comment
    "pkg" // trailing comment
    "#;
    let tokens = tokenize(input);
    expect_test::expect![[r#"
        Ok(
            [
                IMPORT(
                    Pos {
                        line: 3,
                        column: 5,
                    }..Pos {
                        line: 3,
                        column: 11,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 4,
                            column: 5,
                        }..Pos {
                            line: 4,
                            column: 10,
                        },
                        "pkg",
                    ),
                ),
                EOF(
                    Pos {
                        line: 5,
                        column: 5,
                    }..Pos {
                        line: 5,
                        column: 5,
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&tokens);

    // Comments between tokens
    let input = r#"import // comment
    "pkg""#;
    let tokens = tokenize(input);
    expect_test::expect![[r#"
        Ok(
            [
                IMPORT(
                    Pos {
                        line: 1,
                        column: 1,
                    }..Pos {
                        line: 1,
                        column: 7,
                    },
                ),
                STRING(
                    (
                        Pos {
                            line: 2,
                            column: 5,
                        }..Pos {
                            line: 2,
                            column: 10,
                        },
                        "pkg",
                    ),
                ),
                EOF(
                    Pos {
                        line: 2,
                        column: 10,
                    }..Pos {
                        line: 2,
                        column: 10,
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&tokens);
}

#[test]
fn test_escape_sequences() {
    // Test all escape sequences: \n \t \r \f \b \\ \" \/ \uXXXX
    let input = r#""\n\t\f\b\r\\\"\/""#;
    let tokens = tokenize(input);
    expect_test::expect![[r#"
        Ok(
            [
                STRING(
                    (
                        Pos {
                            line: 1,
                            column: 1,
                        }..Pos {
                            line: 1,
                            column: 19,
                        },
                        "\n\t\u{c}\u{8}\r\\\"/",
                    ),
                ),
                EOF(
                    Pos {
                        line: 1,
                        column: 19,
                    }..Pos {
                        line: 1,
                        column: 19,
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&tokens);

    // Test unicode escape \uXXXX
    let input = r#""\u0041\u2665\uFFFF""#;
    let tokens = tokenize(input);
    expect_test::expect![[r#"
        Ok(
            [
                STRING(
                    (
                        Pos {
                            line: 1,
                            column: 1,
                        }..Pos {
                            line: 1,
                            column: 21,
                        },
                        "A♥\u{ffff}",
                    ),
                ),
                EOF(
                    Pos {
                        line: 1,
                        column: 21,
                    }..Pos {
                        line: 1,
                        column: 21,
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&tokens);

    // Test invalid escape sequence should fail
    let input = r#""invalid\x""#;
    let tokens = tokenize(input);
    expect_test::expect![[r#"
        Err(
            "Lexing error at 0..11",
        )
    "#]]
    .assert_debug_eq(&tokens);
}
