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

use anyhow::bail;
use indexmap::IndexSet;

use crate::{
    common::TargetBackend,
    package::{SupportedTargetsConfig, SupportedTargetsDeclKind},
};

pub fn resolve_supported_targets(
    supported_targets: Option<&SupportedTargetsConfig>,
) -> anyhow::Result<(IndexSet<TargetBackend>, SupportedTargetsDeclKind)> {
    match supported_targets {
        None => Ok((
            TargetBackend::all().iter().copied().collect(),
            SupportedTargetsDeclKind::Omitted,
        )),
        Some(SupportedTargetsConfig::LegacyArray(list)) => {
            let mut supported_backends = IndexSet::new();
            for backend in list {
                supported_backends.insert(TargetBackend::str_to_backend(backend)?);
            }
            Ok((supported_backends, SupportedTargetsDeclKind::LegacyArray))
        }
        Some(SupportedTargetsConfig::Expr(expr)) => Ok((
            parse_supported_targets_expr(expr)?,
            SupportedTargetsDeclKind::Expr,
        )),
    }
}

fn parse_supported_targets_expr(expr: &str) -> anyhow::Result<IndexSet<TargetBackend>> {
    const EXPR_HINT: &str = "Valid examples: `js` or `all-js+wasm-gc`.";
    const SUPPORTED_TARGET_TOKENS: [&str; 6] = ["wasm-gc", "native", "wasm", "llvm", "all", "js"];
    let expr = expr.trim();
    if expr.is_empty() {
        bail!(
            "invalid `supported_targets` expression: expression cannot be empty. {}",
            EXPR_HINT
        );
    }

    let bytes = expr.as_bytes();
    let mut i = 0;
    let mut selected = IndexSet::new();
    let mut first_term = true;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let op = match bytes[i] as char {
            '+' | '-' => {
                let op = bytes[i] as char;
                i += 1;
                op
            }
            _ if first_term => '+',
            _ => {
                bail!(
                    "invalid `supported_targets` expression `{}`: expected `+` or `-` at position {}. {}",
                    expr,
                    i,
                    EXPR_HINT
                );
            }
        };
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let token_start = i;
        if token_start >= bytes.len() {
            bail!(
                "invalid `supported_targets` expression `{}`: missing token after `{}` at position {}. {}",
                expr,
                op,
                token_start.saturating_sub(1),
                EXPR_HINT
            );
        }

        let mut token = None;
        for candidate in SUPPORTED_TARGET_TOKENS {
            if !expr[token_start..].starts_with(candidate) {
                continue;
            }
            let token_end = token_start + candidate.len();
            let is_boundary = token_end >= bytes.len()
                || matches!(bytes[token_end] as char, '+' | '-')
                || bytes[token_end].is_ascii_whitespace();
            if is_boundary {
                token = Some(candidate);
                i = token_end;
                break;
            }
        }

        let token = if let Some(token) = token {
            token
        } else {
            let mut token_end = token_start;
            while token_end < bytes.len() {
                let c = bytes[token_end] as char;
                if c == '+' || c == '-' {
                    break;
                }
                token_end += 1;
            }
            let token = expr[token_start..token_end].trim();
            if token.is_empty() {
                bail!(
                    "invalid `supported_targets` expression `{}`: empty token at position {}. {}",
                    expr,
                    token_start,
                    EXPR_HINT
                );
            }
            token
        };

        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if token == "all" {
            if op == '+' {
                selected.extend(TargetBackend::all().iter().copied());
            } else {
                selected.clear();
            }
            first_term = false;
            continue;
        }

        let backend = TargetBackend::str_to_backend(token).map_err(|_| {
            anyhow::anyhow!(
                "invalid `supported_targets` expression `{}`: unknown token `{}`. {}",
                expr,
                token,
                EXPR_HINT
            )
        })?;
        if op == '+' {
            selected.insert(backend);
        } else {
            selected.shift_remove(&backend);
        }
        first_term = false;
    }

    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(set: &IndexSet<TargetBackend>) -> Vec<&'static str> {
        let mut v = set.iter().map(|b| b.to_flag()).collect::<Vec<_>>();
        v.sort();
        v
    }

    #[test]
    fn parse_simple_expr_from_empty_baseline() {
        let set = parse_supported_targets_expr("js").unwrap();
        assert_eq!(flags(&set), vec!["js"]);
    }

    #[test]
    fn parse_wasm_gc_without_splitting_dash() {
        let set = parse_supported_targets_expr("+all-wasm+wasm-gc").unwrap();
        assert_eq!(flags(&set), vec!["js", "llvm", "native", "wasm-gc"]);
    }

    #[test]
    fn parse_remove_from_empty_results_empty() {
        let set = parse_supported_targets_expr("-wasm-gc").unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn parse_all_minus_js() {
        let set = parse_supported_targets_expr("+all-js").unwrap();
        assert_eq!(flags(&set), vec!["llvm", "native", "wasm", "wasm-gc"]);
    }

    #[test]
    fn resolve_none_means_all() {
        let (set, kind) = resolve_supported_targets(None).unwrap();
        assert_eq!(kind, SupportedTargetsDeclKind::Omitted);
        assert_eq!(flags(&set), vec!["js", "llvm", "native", "wasm", "wasm-gc"]);
    }

    #[test]
    fn resolve_legacy_array() {
        let cfg = SupportedTargetsConfig::LegacyArray(vec!["native".into(), "js".into()]);
        let (set, kind) = resolve_supported_targets(Some(&cfg)).unwrap();
        assert_eq!(kind, SupportedTargetsDeclKind::LegacyArray);
        assert_eq!(flags(&set), vec!["js", "native"]);
    }

    #[test]
    fn parse_invalid_token() {
        let err = parse_supported_targets_expr("+foo")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown token `foo`"));
    }
}
