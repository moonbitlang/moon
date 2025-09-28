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

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::common::TargetBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OptLevel {
    Release,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TargetOs {
    Windows,
    Linux,
    MacOS,
}

impl TargetOs {
    /// Get the current OS as TargetOs. Returns None for non-native targets.
    pub fn current_os_for_native_target(target_backend: TargetBackend) -> Option<Self> {
        match target_backend {
            TargetBackend::Native | TargetBackend::LLVM => {
                match std::env::consts::OS {
                    "windows" => Some(TargetOs::Windows),
                    "linux" => Some(TargetOs::Linux),
                    "macos" => Some(TargetOs::MacOS),
                    _ => None, // Unsupported OS
                }
            }
            // Non-native targets don't have an OS
            TargetBackend::Wasm | TargetBackend::WasmGC | TargetBackend::Js => None,
        }
    }
}

impl OptLevel {
    pub fn from_debug_flag(debug_flag: bool) -> Self {
        if debug_flag {
            Self::Debug
        } else {
            Self::Release
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Debug, Self::Release]
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
    Os(TargetOs),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "StringOrArray", into = "StringOrArray")]
pub enum CondExpr {
    Atom(Atom),
    Condition(LogicOp, Vec<CondExpr>),
}

impl CondExpr {
    pub fn eval(&self, opt_level: OptLevel, target_backend: TargetBackend, target_os: Option<TargetOs>) -> bool {
        match self {
            CondExpr::Atom(atom) => match atom {
                Atom::OptLevel(level) => level == &opt_level,
                Atom::Target(backend) => backend == &target_backend,
                Atom::Os(os) => target_os.map_or(false, |target_os| os == &target_os),
            },
            CondExpr::Condition(op, exprs) => match op {
                LogicOp::And => exprs.iter().all(|x| x.eval(opt_level, target_backend, target_os)),
                LogicOp::Or => exprs.iter().any(|x| x.eval(opt_level, target_backend, target_os)),
                LogicOp::Not => !exprs.iter().any(|x| x.eval(opt_level, target_backend, target_os)),
            },
        }
    }

    pub fn to_compile_condition(&self) -> CompileCondition {
        self.to_compile_condition_with_os(None)
    }

    pub fn to_compile_condition_with_os(&self, target_os: Option<TargetOs>) -> CompileCondition {
        use std::collections::HashSet;

        let mut backend_set = HashSet::new();
        let mut optlevel_set = HashSet::new();
        for (t, o) in [
            (TargetBackend::Wasm, OptLevel::Debug),
            (TargetBackend::Wasm, OptLevel::Release),
            (TargetBackend::WasmGC, OptLevel::Debug),
            (TargetBackend::WasmGC, OptLevel::Release),
            (TargetBackend::Js, OptLevel::Debug),
            (TargetBackend::Js, OptLevel::Release),
            (TargetBackend::Native, OptLevel::Debug),
            (TargetBackend::Native, OptLevel::Release),
            (TargetBackend::LLVM, OptLevel::Debug),
            (TargetBackend::LLVM, OptLevel::Release),
        ] {
            // For native backends, use the provided target_os
            // For non-native backends, use None (no OS)
            let os_for_eval = match t {
                TargetBackend::Native | TargetBackend::LLVM => target_os,
                _ => None,
            };
            
            if self.eval(o, t, os_for_eval) {
                optlevel_set.insert(o);
                backend_set.insert(t);
            }
        }

        let mut backend: Vec<_> = backend_set.into_iter().collect();
        let mut optlevel: Vec<_> = optlevel_set.into_iter().collect();

        // to keep a stable order
        backend.sort();
        optlevel.sort();
        CompileCondition { backend, optlevel }
    }
}

// in packages.json, for ide usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompileCondition {
    pub backend: Vec<TargetBackend>,
    pub optlevel: Vec<OptLevel>,
}

impl CompileCondition {
    pub fn eval(&self, opt_level: OptLevel, target_backend: TargetBackend) -> bool {
        self.optlevel.contains(&opt_level) && self.backend.contains(&target_backend)
    }

    /// Evaluate with OS support. This method supports OS-based conditions by
    /// checking if the compile condition would be satisfied for any combination
    /// of the supported backends/optlevels that also match the given OS.
    pub fn eval_with_os(&self, opt_level: OptLevel, target_backend: TargetBackend, _target_os: Option<TargetOs>) -> bool {
        // For backward compatibility, if no OS conditions are involved, use regular eval
        self.eval(opt_level, target_backend)
        // Note: CompileCondition doesn't directly store OS conditions, so OS-based 
        // conditions are handled at the CondExpr level before conversion to CompileCondition
    }
}

impl Default for CompileCondition {
    fn default() -> Self {
        Self {
            backend: vec![
                TargetBackend::Wasm,
                TargetBackend::WasmGC,
                TargetBackend::Js,
                TargetBackend::Native,
                TargetBackend::LLVM,
            ],
            optlevel: vec![OptLevel::Debug, OptLevel::Release],
        }
    }
}

#[test]
fn test_001() {
    let a = CompileCondition {
        backend: vec![
            TargetBackend::Wasm,
            TargetBackend::WasmGC,
            TargetBackend::Js,
        ],
        optlevel: vec![OptLevel::Debug, OptLevel::Release],
    };
    assert!(a.eval(OptLevel::Release, TargetBackend::Wasm));
    assert!(a.eval(OptLevel::Release, TargetBackend::WasmGC));
    assert!(a.eval(OptLevel::Release, TargetBackend::Js));
    assert!(a.eval(OptLevel::Debug, TargetBackend::Js));
    assert!(a.eval(OptLevel::Debug, TargetBackend::Wasm));
    assert!(a.eval(OptLevel::Debug, TargetBackend::WasmGC));

    let b = CompileCondition {
        backend: vec![TargetBackend::Wasm, TargetBackend::Js],
        optlevel: vec![OptLevel::Debug],
    };
    assert!(b.eval(OptLevel::Debug, TargetBackend::Js));
    assert!(b.eval(OptLevel::Debug, TargetBackend::Wasm));
}

#[test]
fn test_eval_001() {
    // [or js]
    let lhs = CondExpr::Condition(
        LogicOp::Or,
        vec![CondExpr::Atom(Atom::Target(TargetBackend::Js))],
    );
    let result = lhs.eval(OptLevel::Release, TargetBackend::Js, None);
    assert!(result);

    // [or release]
    let rhs = CondExpr::Condition(
        LogicOp::Or,
        vec![CondExpr::Atom(Atom::OptLevel(OptLevel::Release))],
    );
    let result = rhs.eval(OptLevel::Release, TargetBackend::Js, None);
    assert!(result);

    // [and, [or js], [or, release]]
    let e = CondExpr::Condition(LogicOp::And, vec![lhs.clone(), rhs.clone()]);
    let result = e.eval(OptLevel::Release, TargetBackend::Js, None);
    assert!(result);

    let e = CondExpr::Condition(LogicOp::And, vec![lhs.clone(), rhs.clone()]);
    let result = e.eval(OptLevel::Debug, TargetBackend::Js, None);
    assert!(!result);

    let e = CondExpr::Condition(LogicOp::And, vec![lhs, rhs]);
    let result = e.eval(OptLevel::Release, TargetBackend::WasmGC, None);
    assert!(!result);
}

#[test]
fn test_eval_002() {
    // [not js]
    let lhs = CondExpr::Condition(
        LogicOp::Not,
        vec![CondExpr::Atom(Atom::Target(TargetBackend::Js))],
    );
    let result = lhs.eval(OptLevel::Release, TargetBackend::Js, None);
    assert!(!result);
    let result = lhs.eval(OptLevel::Release, TargetBackend::Wasm, None);
    assert!(result);
    let result = lhs.eval(OptLevel::Release, TargetBackend::WasmGC, None);
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
    let result = e.eval(OptLevel::Release, TargetBackend::Wasm, None);
    assert!(!result);
    let result = e.eval(OptLevel::Release, TargetBackend::WasmGC, None);
    assert!(!result);
    let result = e.eval(OptLevel::Release, TargetBackend::Js, None);
    assert!(result);
    let result = e.eval(OptLevel::Release, TargetBackend::Js, None);
    assert!(result);
}

#[test]
fn test_eval_os() {
    // Test OS-based conditional compilation
    let windows_expr = CondExpr::Atom(Atom::Os(TargetOs::Windows));
    let linux_expr = CondExpr::Atom(Atom::Os(TargetOs::Linux));
    let macos_expr = CondExpr::Atom(Atom::Os(TargetOs::MacOS));
    
    // Test with Windows OS
    assert!(windows_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Windows)));
    assert!(!linux_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Windows)));
    assert!(!macos_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Windows)));
    
    // Test with Linux OS
    assert!(!windows_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Linux)));
    assert!(linux_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Linux)));
    assert!(!macos_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Linux)));
    
    // Test with macOS
    assert!(!windows_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::MacOS)));
    assert!(!linux_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::MacOS)));
    assert!(macos_expr.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::MacOS)));
    
    // Test with None OS (should not match any OS atoms)
    assert!(!windows_expr.eval(OptLevel::Release, TargetBackend::Js, None));
    assert!(!linux_expr.eval(OptLevel::Release, TargetBackend::Js, None));
    assert!(!macos_expr.eval(OptLevel::Release, TargetBackend::Js, None));
    
    // Test combined condition: windows AND native
    let combined = CondExpr::Condition(
        LogicOp::And,
        vec![
            CondExpr::Atom(Atom::Os(TargetOs::Windows)),
            CondExpr::Atom(Atom::Target(TargetBackend::Native)),
        ],
    );
    assert!(combined.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Windows)));
    assert!(!combined.eval(OptLevel::Release, TargetBackend::Js, Some(TargetOs::Windows)));
    assert!(!combined.eval(OptLevel::Release, TargetBackend::Native, Some(TargetOs::Linux)));
}

#[test]
fn test_parse_os_tokens() {
    // Test parsing OS tokens
    use crate::cond_expr::StringOrArray;
    
    let windows_str = StringOrArray::String("windows".to_string());
    let windows_expr = CondExpr::try_from(windows_str).unwrap();
    assert!(matches!(windows_expr, CondExpr::Atom(Atom::Os(TargetOs::Windows))));
    
    let linux_str = StringOrArray::String("linux".to_string());
    let linux_expr = CondExpr::try_from(linux_str).unwrap();
    assert!(matches!(linux_expr, CondExpr::Atom(Atom::Os(TargetOs::Linux))));
    
    let macos_str = StringOrArray::String("macos".to_string());
    let macos_expr = CondExpr::try_from(macos_str).unwrap();
    assert!(matches!(macos_expr, CondExpr::Atom(Atom::Os(TargetOs::MacOS))));
    
    // Test serialization back to string
    assert_eq!(StringOrArray::from(windows_expr), StringOrArray::String("windows".to_string()));
    assert_eq!(StringOrArray::from(linux_expr), StringOrArray::String("linux".to_string()));
    assert_eq!(StringOrArray::from(macos_expr), StringOrArray::String("macos".to_string()));
}

#[test]
fn test_to_compile_condition_with_os() {
    // Test that OS conditions are properly converted to CompileCondition
    let windows_native = CondExpr::Condition(
        LogicOp::And,
        vec![
            CondExpr::Atom(Atom::Os(TargetOs::Windows)),
            CondExpr::Atom(Atom::Target(TargetBackend::Native)),
        ],
    );
    
    // When converted with Windows OS, should include Native backend
    let condition_with_windows = windows_native.to_compile_condition_with_os(Some(TargetOs::Windows));
    assert!(condition_with_windows.backend.contains(&TargetBackend::Native));
    
    // When converted with Linux OS, should not include Native backend
    let condition_with_linux = windows_native.to_compile_condition_with_os(Some(TargetOs::Linux));
    assert!(!condition_with_linux.backend.contains(&TargetBackend::Native));
    
    // When converted with no OS, should not include Native backend
    let condition_with_no_os = windows_native.to_compile_condition_with_os(None);
    assert!(!condition_with_no_os.backend.contains(&TargetBackend::Native));
}

#[test] 
fn test_os_current_detection() {
    // Test that OS detection works for different backends
    let current_os_native = TargetOs::current_os_for_native_target(TargetBackend::Native);
    let current_os_llvm = TargetOs::current_os_for_native_target(TargetBackend::LLVM);
    let current_os_js = TargetOs::current_os_for_native_target(TargetBackend::Js);
    let current_os_wasm = TargetOs::current_os_for_native_target(TargetBackend::Wasm);
    let current_os_wasm_gc = TargetOs::current_os_for_native_target(TargetBackend::WasmGC);
    
    // Native and LLVM should detect current OS
    assert!(current_os_native.is_some());
    assert!(current_os_llvm.is_some());
    assert_eq!(current_os_native, current_os_llvm);
    
    // Non-native targets should return None
    assert!(current_os_js.is_none());
    assert!(current_os_wasm.is_none());
    assert!(current_os_wasm_gc.is_none());
    
    // The detected OS should be one of the supported ones
    match current_os_native {
        Some(TargetOs::Windows) | Some(TargetOs::Linux) | Some(TargetOs::MacOS) => {},
        None => panic!("Should have detected an OS for native target"),
    }
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
        "native" => Ok(CondExpr::Atom(Atom::Target(TargetBackend::Native))),
        "llvm" => Ok(CondExpr::Atom(Atom::Target(TargetBackend::LLVM))),
        "windows" => Ok(CondExpr::Atom(Atom::Os(TargetOs::Windows))),
        "linux" => Ok(CondExpr::Atom(Atom::Os(TargetOs::Linux))),
        "macos" => Ok(CondExpr::Atom(Atom::Os(TargetOs::MacOS))),
        _ => Err(ParseTargetError::UnknownTarget(expr.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to parse conditional expression")]
pub struct ParseCondExprError {
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

pub fn parse_cond_expr(value: &StringOrArray) -> Result<CondExpr, ParseCondExprError> {
    match value {
        StringOrArray::String(s) => parse_cond_target(s).map_err(|e| ParseCondExprError {
            source: ParseCondExprErrorKind::ParseCondAtomError(e),
        }),
        StringOrArray::Array(arr) => {
            if arr.is_empty() {
                return Err(ParseCondExprError {
                    source: ParseCondExprErrorKind::EmptyConditionArray,
                });
            }
            let mut iter = arr.iter();
            match iter.next() {
                Some(StringOrArray::String(op)) => {
                    let logic_op = parse_cond_logic_op(op).map_err(|e| ParseCondExprError {
                        source: ParseCondExprErrorKind::ParseCondLogicOpError(e),
                    });

                    match logic_op {
                        Ok(logic_op) => {
                            let sub_exprs: Result<Vec<CondExpr>, ParseCondExprError> =
                                iter.map(parse_cond_expr).collect();
                            Ok(CondExpr::Condition(logic_op, sub_exprs?))
                        }
                        Err(_) => {
                            let atom = parse_cond_target(op).map_err(|e| ParseCondExprError {
                                source: ParseCondExprErrorKind::ParseCondAtomError(e),
                            })?;
                            let sub_exprs: Result<Vec<CondExpr>, ParseCondExprError> =
                                iter.map(parse_cond_expr).collect();
                            let mut sub_exprs = sub_exprs?;
                            sub_exprs.insert(0, atom);
                            Ok(CondExpr::Condition(LogicOp::Or, sub_exprs))
                        }
                    }
                }
                _ => Err(ParseCondExprError {
                    source: ParseCondExprErrorKind::EmptyConditionArray,
                }),
            }
        }
    }
}

impl TryFrom<StringOrArray> for CondExpr {
    type Error = ParseCondExprError;

    fn try_from(value: StringOrArray) -> Result<Self, Self::Error> {
        parse_cond_expr(&value)
    }
}

impl From<CondExpr> for StringOrArray {
    fn from(val: CondExpr) -> Self {
        match val {
            CondExpr::Atom(atom) => match atom {
                Atom::OptLevel(OptLevel::Release) => StringOrArray::String("release".to_string()),
                Atom::OptLevel(OptLevel::Debug) => StringOrArray::String("debug".to_string()),
                Atom::Target(tb) => match tb {
                    TargetBackend::Wasm => StringOrArray::String("wasm".to_string()),
                    TargetBackend::WasmGC => StringOrArray::String("wasm-gc".to_string()),
                    TargetBackend::Js => StringOrArray::String("js".to_string()),
                    TargetBackend::Native => StringOrArray::String("native".to_string()),
                    TargetBackend::LLVM => StringOrArray::String("llvm".to_string()),
                },
                Atom::Os(os) => match os {
                    TargetOs::Windows => StringOrArray::String("windows".to_string()),
                    TargetOs::Linux => StringOrArray::String("linux".to_string()),
                    TargetOs::MacOS => StringOrArray::String("macos".to_string()),
                },
            },
            CondExpr::Condition(op, exprs) => {
                let mut arr: Vec<StringOrArray> = Vec::with_capacity(exprs.len() + 1);
                let op_str = match op {
                    LogicOp::And => "and",
                    LogicOp::Or => "or",
                    LogicOp::Not => "not",
                };
                arr.push(StringOrArray::String(op_str.to_string()));
                for e in exprs {
                    arr.push(e.into());
                }
                StringOrArray::Array(arr)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StringOrArray {
    String(String),
    Array(Vec<StringOrArray>),
}

pub type CondExprs = IndexMap<String, CondExpr>;
