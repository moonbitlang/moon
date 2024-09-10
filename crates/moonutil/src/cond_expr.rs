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

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::common::TargetBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptLevel {
    Release,
    Debug,
}

impl OptLevel {
    pub fn from_debug_flag(debug_flag: bool) -> Self {
        if debug_flag {
            Self::Debug
        } else {
            Self::Release
        }
    }
}

#[derive(Debug, Clone)]
pub enum LogicOp {
    And,
    Or,
    Not,
}

#[derive(Debug, Clone)]
pub enum Atom {
    OptLevel(OptLevel),
    Target(TargetBackend),
}

#[derive(Debug, Clone)]
pub enum CondExpr {
    Atom(Atom),
    Condition(LogicOp, Vec<CondExpr>),
}

impl CondExpr {
    pub fn eval(&self, opt_level: OptLevel, target_backend: TargetBackend) -> bool {
        match self {
            CondExpr::Atom(atom) => match atom {
                Atom::OptLevel(level) => level == &opt_level,
                Atom::Target(backend) => backend == &target_backend,
            },
            CondExpr::Condition(op, exprs) => match op {
                LogicOp::And => exprs.iter().all(|x| x.eval(opt_level, target_backend)),
                LogicOp::Or => exprs.iter().any(|x| x.eval(opt_level, target_backend)),
                LogicOp::Not => !exprs.iter().any(|x| x.eval(opt_level, target_backend)),
            },
        }
    }
}

#[test]
fn test_eval_001() {
    // [or js]
    let lhs = CondExpr::Condition(
        LogicOp::Or,
        vec![CondExpr::Atom(Atom::Target(TargetBackend::Js))],
    );

    let result = lhs.eval(OptLevel::Release, TargetBackend::Js);
    assert!(result);

    // [or release]
    let rhs = CondExpr::Condition(
        LogicOp::Or,
        vec![CondExpr::Atom(Atom::OptLevel(OptLevel::Release))],
    );
    let result = rhs.eval(OptLevel::Release, TargetBackend::Js);
    assert!(result);

    // [and, [or js], [or, release]]
    let e = CondExpr::Condition(LogicOp::And, vec![lhs.clone(), rhs.clone()]);
    let result = e.eval(OptLevel::Release, TargetBackend::Js);
    assert!(result);

    let e = CondExpr::Condition(LogicOp::And, vec![lhs.clone(), rhs.clone()]);
    let result = e.eval(OptLevel::Debug, TargetBackend::Js);
    assert!(!result);

    let e = CondExpr::Condition(LogicOp::And, vec![lhs, rhs]);
    let result = e.eval(OptLevel::Release, TargetBackend::WasmGC);
    assert!(!result);
}

#[test]
fn test_eval_002() {
    // [not js]
    let lhs = CondExpr::Condition(
        LogicOp::Not,
        vec![CondExpr::Atom(Atom::Target(TargetBackend::Js))],
    );
    let result = lhs.eval(OptLevel::Release, TargetBackend::Js);
    assert!(!result);
    let result = lhs.eval(OptLevel::Release, TargetBackend::Wasm);
    assert!(result);
    let result = lhs.eval(OptLevel::Release, TargetBackend::WasmGC);
    assert!(result);
}

#[test]
fn test_eval_003() {
    // [not wasm wasm-gc]
    let e = CondExpr::Condition(
        LogicOp::Not,
        vec![
            CondExpr::Atom(Atom::Target(TargetBackend::Wasm)),
            CondExpr::Atom(Atom::Target(TargetBackend::WasmGC)),
        ],
    );
    let result = e.eval(OptLevel::Release, TargetBackend::Wasm);
    assert!(!result);
    let result = e.eval(OptLevel::Release, TargetBackend::WasmGC);
    assert!(!result);
    let result = e.eval(OptLevel::Release, TargetBackend::Js);
    assert!(result);
    let result = e.eval(OptLevel::Release, TargetBackend::Js);
    assert!(result);
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseLogicOpError {
    #[error("empty string")]
    EmptyString,
    #[error("unknown logic operator: {0}")]
    UnknownLogicOp(String),
}

pub fn parse_cond_logic_op(expr: &str) -> Result<LogicOp, ParseLogicOpError> {
    match expr {
        "and" => Ok(LogicOp::And),
        "or" => Ok(LogicOp::Or),
        "not" => Ok(LogicOp::Not),
        "" => Err(ParseLogicOpError::EmptyString),
        _ => Err(ParseLogicOpError::UnknownLogicOp(expr.to_string())),
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseTargetError {
    #[error("empty string")]
    EmptyString,
    #[error("unknown target: {0}")]
    UnknownTarget(String),
}

pub fn parse_cond_target(expr: &str) -> Result<CondExpr, ParseTargetError> {
    if expr.is_empty() {
        return Err(ParseTargetError::EmptyString);
    }
    match expr {
        "release" => Ok(CondExpr::Atom(Atom::OptLevel(OptLevel::Release))),
        "debug" => Ok(CondExpr::Atom(Atom::OptLevel(OptLevel::Debug))),
        "wasm" => Ok(CondExpr::Atom(Atom::Target(TargetBackend::Wasm))),
        "wasm-gc" => Ok(CondExpr::Atom(Atom::Target(TargetBackend::WasmGC))),
        "js" => Ok(CondExpr::Atom(Atom::Target(TargetBackend::Js))),
        _ => Err(ParseTargetError::UnknownTarget(expr.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to parse conditional expression in file: {file}")]
pub struct ParseCondExprError {
    pub file: PathBuf,
    #[source]
    source: ParseCondExprErrorKind,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseCondExprErrorKind {
    #[error("failed to parse atom expression")]
    ParseCondAtomError(#[from] ParseTargetError),
    #[error("failed to parse logic operator ")]
    ParseCondLogicOpError(#[from] ParseLogicOpError),
    #[error("empty condition array")]
    EmptyConditionArray,
}

pub fn parse_cond_exprs(file: &Path, map: &RawTargets) -> Result<CondExprs, ParseCondExprError> {
    map.iter()
        .map(|(k, v)| {
            let cond_expr = parse_cond_expr(file, v)?;
            Ok((k.clone(), cond_expr))
        })
        .collect()
}

fn parse_cond_expr(file: &Path, value: &StringOrArray) -> Result<CondExpr, ParseCondExprError> {
    match value {
        StringOrArray::String(s) => parse_cond_target(s).map_err(|e| ParseCondExprError {
            file: file.to_path_buf(),
            source: ParseCondExprErrorKind::ParseCondAtomError(e),
        }),
        StringOrArray::Array(arr) => {
            if arr.is_empty() {
                return Err(ParseCondExprError {
                    file: file.to_path_buf(),
                    source: ParseCondExprErrorKind::EmptyConditionArray,
                });
            }
            let mut iter = arr.iter();
            match iter.next() {
                Some(StringOrArray::String(op)) => {
                    let logic_op = parse_cond_logic_op(op).map_err(|e| ParseCondExprError {
                        file: file.to_path_buf(),
                        source: ParseCondExprErrorKind::ParseCondLogicOpError(e),
                    });

                    match logic_op {
                        Ok(logic_op) => {
                            let sub_exprs: Result<Vec<CondExpr>, ParseCondExprError> =
                                iter.map(|x| parse_cond_expr(file, x)).collect();
                            Ok(CondExpr::Condition(logic_op, sub_exprs?))
                        }
                        Err(_) => {
                            let atom = parse_cond_target(op).map_err(|e| ParseCondExprError {
                                file: file.to_path_buf(),
                                source: ParseCondExprErrorKind::ParseCondAtomError(e),
                            })?;
                            let sub_exprs: Result<Vec<CondExpr>, ParseCondExprError> =
                                iter.map(|x| parse_cond_expr(file, x)).collect();
                            let mut sub_exprs = sub_exprs?;
                            sub_exprs.insert(0, atom);
                            Ok(CondExpr::Condition(LogicOp::Or, sub_exprs))
                        }
                    }
                }
                _ => Err(ParseCondExprError {
                    file: file.to_path_buf(),
                    source: ParseCondExprErrorKind::EmptyConditionArray,
                }),
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray {
    String(String),
    Array(Vec<StringOrArray>),
}

pub type RawTargets = IndexMap<String, StringOrArray>;

pub type CondExprs = IndexMap<String, CondExpr>;
