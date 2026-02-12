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

pub fn demangle_mangled_function_name(func_name: &str) -> String {
    demangle_mangled_function_name_impl(func_name).unwrap_or_else(|| func_name.to_string())
}

#[derive(Debug)]
enum DemangledSymbol {
    Function {
        pkg: String,
        name: String,
        nested: Vec<String>,
        anonymous_index: Option<String>,
        type_args: Option<TypeArgs>,
    },
    Method {
        pkg: String,
        type_name: String,
        method_name: String,
        type_args: Option<TypeArgs>,
    },
    TraitImplMethod {
        impl_type: TypePath,
        trait_type: TypePath,
        method_name: String,
        type_args: Option<TypeArgs>,
    },
    ExtensionMethod {
        type_pkg: String,
        type_name: String,
        method_pkg: String,
        method_name: String,
        type_args: Option<TypeArgs>,
    },
    Type {
        type_path: TypePath,
    },
    Local {
        ident: String,
        stamp: String,
    },
}

#[derive(Debug)]
struct TypePath {
    pkg: String,
    type_name: String,
}

#[derive(Debug)]
struct TypeArgs {
    args: Vec<TypeExpr>,
    raises: Option<TypeExpr>,
}

#[derive(Debug)]
enum TypeExpr {
    Builtin(&'static str),
    FixedArray(Box<TypeExpr>),
    Option(Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Fn {
        async_mark: bool,
        params: Vec<TypeExpr>,
        ret: Box<TypeExpr>,
        raises: Option<Box<TypeExpr>>,
    },
    TypeRef {
        path: TypePath,
        type_args: Option<Box<TypeArgs>>,
    },
}

fn demangle_mangled_function_name_impl(func_name: &str) -> Option<String> {
    let (symbol, j) = parse_mangled_symbol(func_name)?;
    if j < func_name.len() {
        match byte_at(func_name, j) {
            Some(b'.' | b'$' | b'@') => {}
            _ => return None,
        }
    }
    Some(render_symbol(&symbol))
}

fn parse_mangled_symbol(func_name: &str) -> Option<(DemangledSymbol, usize)> {
    if func_name.is_empty() {
        return None;
    }

    let mut i = 0usize;
    if byte_at(func_name, 0) == Some(b'$') {
        i = 1;
    }
    if func_name.len().saturating_sub(i) < 3 {
        return None;
    }
    if byte_at(func_name, i) != Some(b'_')
        || byte_at(func_name, i + 1) != Some(b'M')
        || byte_at(func_name, i + 2) != Some(b'0')
    {
        return None;
    }
    i += 3;
    if i >= func_name.len() {
        return None;
    }

    let tag = byte_at(func_name, i)?;
    i += 1;

    match tag {
        b'F' => parse_tag_f(func_name, i),
        b'M' => parse_tag_m(func_name, i),
        b'I' => parse_tag_i(func_name, i),
        b'E' => parse_tag_e(func_name, i),
        b'T' => parse_tag_t(func_name, i),
        b'L' => parse_tag_l(func_name, i),
        _ => None,
    }
}

fn parse_tag_f(s: &str, i: usize) -> Option<(DemangledSymbol, usize)> {
    let (pkg, pkg_end) = parse_package(s, i)?;
    let (name, mut j) = parse_identifier(s, pkg_end)?;
    let mut nested = Vec::new();

    while byte_at(s, j) == Some(b'N') {
        let (nested_name, nested_end) = parse_identifier(s, j + 1)?;
        nested.push(nested_name);
        j = nested_end;
    }

    let mut anonymous_index = None;
    if byte_at(s, j) == Some(b'C') {
        j += 1;
        let start = j;
        while byte_at(s, j).is_some_and(is_digit) {
            j += 1;
        }
        if start == j {
            return None;
        }
        anonymous_index = Some(s[start..j].to_string());
    }

    let (type_args, j) = parse_optional_type_args_ast(s, j)?;
    Some((
        DemangledSymbol::Function {
            pkg,
            name,
            nested,
            anonymous_index,
            type_args,
        },
        j,
    ))
}

fn parse_tag_m(s: &str, i: usize) -> Option<(DemangledSymbol, usize)> {
    let (pkg, pkg_end) = parse_package(s, i)?;
    let (type_name, type_end) = parse_identifier(s, pkg_end)?;
    let (method_name, method_end) = parse_identifier(s, type_end)?;
    let (type_args, j) = parse_optional_type_args_ast(s, method_end)?;

    Some((
        DemangledSymbol::Method {
            pkg,
            type_name,
            method_name,
            type_args,
        },
        j,
    ))
}

fn parse_tag_i(s: &str, i: usize) -> Option<(DemangledSymbol, usize)> {
    let (impl_type, impl_end) = parse_type_path(s, i, false)?;
    let (trait_type, trait_end) = parse_type_path(s, impl_end, false)?;
    let (method_name, method_end) = parse_identifier(s, trait_end)?;
    let (type_args, j) = parse_optional_type_args_ast(s, method_end)?;

    Some((
        DemangledSymbol::TraitImplMethod {
            impl_type,
            trait_type,
            method_name,
            type_args,
        },
        j,
    ))
}

fn parse_tag_e(s: &str, i: usize) -> Option<(DemangledSymbol, usize)> {
    let (type_pkg, type_pkg_end) = parse_package(s, i)?;
    let (type_name, type_name_end) = parse_identifier(s, type_pkg_end)?;
    let (method_pkg, method_pkg_end) = parse_package(s, type_name_end)?;
    let (method_name, method_name_end) = parse_identifier(s, method_pkg_end)?;
    let (type_args, j) = parse_optional_type_args_ast(s, method_name_end)?;

    Some((
        DemangledSymbol::ExtensionMethod {
            type_pkg,
            type_name,
            method_pkg,
            method_name,
            type_args,
        },
        j,
    ))
}

fn parse_tag_t(s: &str, i: usize) -> Option<(DemangledSymbol, usize)> {
    let (type_path, j) = parse_type_path(s, i, false)?;
    Some((DemangledSymbol::Type { type_path }, j))
}

fn parse_tag_l(s: &str, i: usize) -> Option<(DemangledSymbol, usize)> {
    let mut j = i;
    if byte_at(s, j) == Some(b'm') {
        j += 1;
    }

    let (ident, ident_end) = parse_identifier(s, j)?;
    j = ident_end;

    if byte_at(s, j) != Some(b'S') {
        return None;
    }
    j += 1;
    if !byte_at(s, j).is_some_and(is_digit) {
        return None;
    }
    let stamp_start = j;
    while byte_at(s, j).is_some_and(is_digit) {
        j += 1;
    }
    let stamp = s[stamp_start..j].to_string();

    Some((DemangledSymbol::Local { ident, stamp }, j))
}

fn parse_type_path(s: &str, i: usize, omit_core_prefix: bool) -> Option<(TypePath, usize)> {
    let (mut pkg, pkg_end) = parse_package(s, i)?;
    let (mut type_name, mut k) = parse_identifier(s, pkg_end)?;

    if byte_at(s, k) == Some(b'L') {
        let (local, local_end) = parse_identifier(s, k + 1)?;
        type_name.push('.');
        type_name.push_str(&local);
        k = local_end;
    }

    if omit_core_prefix && is_core_package(&pkg) {
        pkg.clear();
    }

    Some((TypePath { pkg, type_name }, k))
}

fn parse_type_ref_ast(s: &str, i: usize) -> Option<(TypeExpr, usize)> {
    if byte_at(s, i) != Some(b'R') {
        return None;
    }
    let (path, mut j) = parse_type_path(s, i + 1, false)?;
    let mut type_args = None;
    if byte_at(s, j) == Some(b'G') {
        let (args, args_end) = parse_type_args_ast(s, j)?;
        type_args = Some(Box::new(args));
        j = args_end;
    }
    Some((TypeExpr::TypeRef { path, type_args }, j))
}

fn parse_fn_type_ast(s: &str, i: usize, async_mark: bool) -> Option<(TypeExpr, usize)> {
    if byte_at(s, i) != Some(b'W') {
        return None;
    }

    let mut j = i + 1;
    let mut params = Vec::new();
    while byte_at(s, j) != Some(b'E') {
        let (param, param_end) = parse_type_expr_ast(s, j)?;
        params.push(param);
        j = param_end;
    }
    j += 1;

    let (ret, ret_end) = parse_type_expr_ast(s, j)?;
    j = ret_end;

    let mut raises = None;
    if byte_at(s, j) == Some(b'Q') {
        let (raised, raised_end) = parse_type_expr_ast(s, j + 1)?;
        raises = Some(Box::new(raised));
        j = raised_end;
    }

    Some((
        TypeExpr::Fn {
            async_mark,
            params,
            ret: Box::new(ret),
            raises,
        },
        j,
    ))
}

fn parse_type_args_ast(s: &str, i: usize) -> Option<(TypeArgs, usize)> {
    let mut j = i;
    let mut args = Vec::new();

    if byte_at(s, j) == Some(b'G') {
        j += 1;
        while byte_at(s, j) != Some(b'E') {
            let (arg, arg_end) = parse_type_expr_ast(s, j)?;
            args.push(arg);
            j = arg_end;
        }
        j += 1;
    }

    let mut raises = None;
    if byte_at(s, j) == Some(b'H') {
        let (raised, raised_end) = parse_type_expr_ast(s, j + 1)?;
        raises = Some(raised);
        j = raised_end;
    }

    Some((TypeArgs { args, raises }, j))
}

fn parse_optional_type_args_ast(s: &str, i: usize) -> Option<(Option<TypeArgs>, usize)> {
    if matches!(byte_at(s, i), Some(b'G' | b'H')) {
        let (args, j) = parse_type_args_ast(s, i)?;
        Some((Some(args), j))
    } else {
        Some((None, i))
    }
}

fn parse_type_expr_ast(s: &str, i: usize) -> Option<(TypeExpr, usize)> {
    let c = byte_at(s, i)?;
    match c {
        b'i' => Some((TypeExpr::Builtin("Int"), i + 1)),
        b'l' => Some((TypeExpr::Builtin("Int64"), i + 1)),
        b'h' => Some((TypeExpr::Builtin("Int16"), i + 1)),
        b'j' => Some((TypeExpr::Builtin("UInt"), i + 1)),
        b'k' => Some((TypeExpr::Builtin("UInt16"), i + 1)),
        b'm' => Some((TypeExpr::Builtin("UInt64"), i + 1)),
        b'd' => Some((TypeExpr::Builtin("Double"), i + 1)),
        b'f' => Some((TypeExpr::Builtin("Float"), i + 1)),
        b'b' => Some((TypeExpr::Builtin("Bool"), i + 1)),
        b'c' => Some((TypeExpr::Builtin("Char"), i + 1)),
        b's' => Some((TypeExpr::Builtin("String"), i + 1)),
        b'u' => Some((TypeExpr::Builtin("Unit"), i + 1)),
        b'y' => Some((TypeExpr::Builtin("Byte"), i + 1)),
        b'z' => Some((TypeExpr::Builtin("Bytes"), i + 1)),
        b'A' => {
            let (inner, inner_end) = parse_type_expr_ast(s, i + 1)?;
            Some((TypeExpr::FixedArray(Box::new(inner)), inner_end))
        }
        b'O' => {
            let (inner, inner_end) = parse_type_expr_ast(s, i + 1)?;
            Some((TypeExpr::Option(Box::new(inner)), inner_end))
        }
        b'U' => {
            let mut j = i + 1;
            let mut elems = Vec::new();
            while byte_at(s, j) != Some(b'E') {
                let (elem, elem_end) = parse_type_expr_ast(s, j)?;
                elems.push(elem);
                j = elem_end;
            }
            Some((TypeExpr::Tuple(elems), j + 1))
        }
        b'V' => parse_fn_type_ast(s, i + 1, true),
        b'W' => parse_fn_type_ast(s, i, false),
        b'R' => parse_type_ref_ast(s, i),
        _ => None,
    }
}

fn render_symbol(symbol: &DemangledSymbol) -> String {
    match symbol {
        DemangledSymbol::Function {
            pkg,
            name,
            nested,
            anonymous_index,
            type_args,
        } => {
            let mut text = if pkg.is_empty() {
                format!("@{name}")
            } else {
                format!("@{pkg}.{name}")
            };
            for nested_name in nested {
                text.push('.');
                text.push_str(nested_name);
            }
            if let Some(idx) = anonymous_index {
                text.push_str(&format!(".{idx} (the {idx}-th anonymous-function)"));
            }
            if let Some(type_args) = type_args {
                text.push_str(&render_type_args(type_args));
            }
            text
        }
        DemangledSymbol::Method {
            pkg,
            type_name,
            method_name,
            type_args,
        } => {
            let mut text = if pkg.is_empty() {
                format!("@{type_name}::{method_name}")
            } else {
                format!("@{pkg}.{type_name}::{method_name}")
            };
            if let Some(type_args) = type_args {
                text.push_str(&render_type_args(type_args));
            }
            text
        }
        DemangledSymbol::TraitImplMethod {
            impl_type,
            trait_type,
            method_name,
            type_args,
        } => {
            let mut text = format!(
                "impl {} for {}",
                render_type_path(trait_type),
                render_type_path(impl_type)
            );
            if let Some(type_args) = type_args {
                text.push_str(&render_type_args(type_args));
            }
            text.push_str(&format!(" with {method_name}"));
            text
        }
        DemangledSymbol::ExtensionMethod {
            type_pkg,
            type_name,
            method_pkg,
            method_name,
            type_args,
        } => {
            let type_pkg_use = if is_core_package(type_pkg) {
                ""
            } else {
                type_pkg
            };
            let method_prefix = if method_pkg.is_empty() {
                String::new()
            } else {
                format!("{method_pkg}.")
            };
            let type_prefix = if type_pkg_use.is_empty() {
                String::new()
            } else {
                format!("{type_pkg_use}.")
            };
            let mut text = format!("@{method_prefix}{type_prefix}{type_name}::{method_name}");
            if let Some(type_args) = type_args {
                text.push_str(&render_type_args(type_args));
            }
            text
        }
        DemangledSymbol::Type { type_path } => render_type_path(type_path),
        DemangledSymbol::Local { ident, stamp } => {
            let no_dollar = ident.strip_prefix('$').unwrap_or(ident);
            let shown = strip_suffix(no_dollar, ".fn");
            format!("{shown}/{stamp}")
        }
    }
}

fn render_type_path(path: &TypePath) -> String {
    if path.pkg.is_empty() {
        format!("@{}", path.type_name)
    } else {
        format!("@{}.{}", path.pkg, path.type_name)
    }
}

fn render_type_args(type_args: &TypeArgs) -> String {
    let mut text = String::new();
    if !type_args.args.is_empty() {
        let rendered_args = type_args
            .args
            .iter()
            .map(render_type_expr)
            .collect::<Vec<_>>()
            .join(", ");
        text.push('[');
        text.push_str(&rendered_args);
        text.push(']');
    }
    if let Some(raises) = &type_args.raises {
        text.push_str(" raise ");
        text.push_str(&render_type_expr(raises));
    }
    text
}

fn render_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Builtin(name) => (*name).to_string(),
        TypeExpr::FixedArray(inner) => format!("FixedArray[{}]", render_type_expr(inner)),
        TypeExpr::Option(inner) => format!("Option[{}]", render_type_expr(inner)),
        TypeExpr::Tuple(elems) => format!(
            "({})",
            elems
                .iter()
                .map(render_type_expr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExpr::Fn {
            async_mark,
            params,
            ret,
            raises,
        } => {
            let prefix = if *async_mark { "async " } else { "" };
            let params = params
                .iter()
                .map(render_type_expr)
                .collect::<Vec<_>>()
                .join(", ");
            let mut text = format!("{prefix}({params}) -> {}", render_type_expr(ret));
            if let Some(raises) = raises {
                text.push_str(" raise ");
                text.push_str(&render_type_expr(raises));
            }
            text
        }
        TypeExpr::TypeRef { path, type_args } => {
            let mut text = render_type_path(path);
            if let Some(type_args) = type_args {
                text.push_str(&render_type_args(type_args));
            }
            text
        }
    }
}

fn parse_package(s: &str, mut i: usize) -> Option<(String, usize)> {
    if byte_at(s, i) != Some(b'P') {
        return None;
    }
    i += 1;

    if byte_at(s, i) == Some(b'B') {
        return Some(("moonbitlang/core/builtin".to_string(), i + 1));
    }

    if byte_at(s, i) == Some(b'C') {
        let (suffix, end) = parse_counted_package_segments(s, i + 1)?;
        let full = if suffix.is_empty() {
            "moonbitlang/core".to_string()
        } else {
            format!("moonbitlang/core/{suffix}")
        };
        return Some((full, end));
    }

    parse_counted_package_segments(s, i)
}

fn parse_counted_package_segments(s: &str, i: usize) -> Option<(String, usize)> {
    let (count, j) = parse_u32(s, i)?;
    if let Some(pkg) = parse_package_segments(s, j, count) {
        return Some(pkg);
    }

    // Backward-compatible fallback: single-digit package segment count.
    let digit = byte_at(s, i)?;
    if !is_digit(digit) {
        return None;
    }
    parse_package_segments(s, i + 1, (digit - b'0') as u32)
}

fn parse_package_segments(s: &str, mut i: usize, count: u32) -> Option<(String, usize)> {
    let mut segs = Vec::new();
    for _ in 0..count {
        let (seg, seg_end) = parse_identifier(s, i)?;
        segs.push(seg);
        i = seg_end;
    }
    Some((segs.join("/"), i))
}

fn parse_identifier(s: &str, i: usize) -> Option<(String, usize)> {
    let (n, start) = parse_u32(s, i)?;
    let n = usize::try_from(n).ok()?;
    let end = start.checked_add(n)?;
    let raw = s.as_bytes().get(start..end)?;

    let out = decode_identifier_bytes(raw)?;
    Some((out, end))
}

fn parse_u32(s: &str, mut i: usize) -> Option<(u32, usize)> {
    if !byte_at(s, i).is_some_and(is_digit) {
        return None;
    }

    let mut v = 0u32;
    while let Some(c) = byte_at(s, i) {
        if !is_digit(c) {
            break;
        }
        v = v.checked_mul(10)?.checked_add((c - b'0') as u32)?;
        i += 1;
    }
    Some((v, i))
}

fn hex_value(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'a'..=b'f' => Some(10 + ch - b'a'),
        b'A'..=b'F' => Some(10 + ch - b'A'),
        _ => None,
    }
}

fn is_core_package(pkg: &str) -> bool {
    let prefix = "moonbitlang/core";
    pkg == prefix
        || pkg
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn strip_suffix<'a>(s: &'a str, suffix: &str) -> &'a str {
    s.strip_suffix(suffix).unwrap_or(s)
}

fn is_digit(ch: u8) -> bool {
    ch.is_ascii_digit()
}

fn byte_at(s: &str, i: usize) -> Option<u8> {
    s.as_bytes().get(i).copied()
}

fn decode_identifier_bytes(raw: &[u8]) -> Option<String> {
    let mut out = String::new();
    let mut k = 0usize;
    while k < raw.len() {
        let c = raw[k];
        if c != b'_' {
            out.push(char::from(c));
            k += 1;
            continue;
        }

        let next = *raw.get(k + 1)?;
        if next == b'_' {
            out.push('_');
            k += 2;
            continue;
        }

        let hi = hex_value(next)?;
        let lo = hex_value(*raw.get(k + 2)?)?;
        out.push(char::from((hi << 4) | lo));
        k += 3;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::demangle_mangled_function_name;

    #[test]
    fn demangle_tags() {
        assert_eq!(demangle_mangled_function_name("_M0FP13pkg3foo"), "@pkg.foo");
        assert_eq!(
            demangle_mangled_function_name("_M0MP13pkg4Type3bar"),
            "@pkg.Type::bar"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0IP13pkg4ImplP13pkg5Trait3run"),
            "impl @pkg.Trait for @pkg.Impl with run"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0EP13pkg4TypeP14util3new"),
            "@util.pkg.Type::new"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0TP13pkg4Type"),
            "@pkg.Type"
        );
        assert_eq!(demangle_mangled_function_name("_M0L3fooS0"), "foo/0");
        assert_eq!(demangle_mangled_function_name("_M0Lm7$foo.fnS12"), "foo/12");
    }

    #[test]
    fn demangle_type_args_and_identifier_escapes() {
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg3fooGiE"),
            "@pkg.foo[Int]"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg4a__b"),
            "@pkg.a_b"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg5a_2db"),
            "@pkg.a-b"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0EP211moonbitlang4core4TypeP14util3new"),
            "@util.Type::new"
        );
    }

    #[test]
    fn demangle_name_mangling_reference_elements() {
        assert_eq!(
            demangle_mangled_function_name("_M0FPB5print"),
            "@moonbitlang/core/builtin.print"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0TPC14list4List"),
            "@moonbitlang/core/list.List"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp5outerN5inner"),
            "@myapp.outer.inner"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp5outerC0"),
            "@myapp.outer.0 (the 0-th anonymous-function)"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0TP15myapp5outerL5Local"),
            "@myapp.outer.Local"
        );
        assert_eq!(
            demangle_mangled_function_name(
                "_M0IP05outerL5LocalP311moonbitlang4core7builtin7Default7defaultGiE"
            ),
            "impl @moonbitlang/core/builtin.Default for @outer.Local[Int] with default"
        );
        assert_eq!(demangle_mangled_function_name("_M0L1xS123"), "x/123");
        assert_eq!(demangle_mangled_function_name("_M0Lm1yS124"), "y/124");
        assert_eq!(
            demangle_mangled_function_name("_M0L6_2atmpS9127"),
            "*tmp/9127"
        );
        assert_eq!(demangle_mangled_function_name("_M0FP03foo"), "@foo");
    }

    #[test]
    fn demangle_name_mangling_reference_type_args() {
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp3zipGisE"),
            "@myapp.zip[Int, String]"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp8try__mapGiEHRP15myapp7MyError"),
            "@myapp.try_map[Int] raise @myapp.MyError"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp5applyGWiEsE"),
            "@myapp.apply[(Int) -> String]"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp3runGVWiEsE"),
            "@myapp.run[async (Int) -> String]"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp8try__runGWiEsQRP15myapp7MyErrorE"),
            "@myapp.try_run[(Int) -> String raise @myapp.MyError]"
        );
        assert_eq!(
            demangle_mangled_function_name(
                "_M0FP15myapp7complexGARP311moonbitlang4core4list4ListGiEE"
            ),
            "@myapp.complex[FixedArray[@moonbitlang/core/list.List[Int]]]"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0EP311moonbitlang4core7builtin3IntP15myapp6double"),
            "@myapp.Int::double"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP28my_2dorg8my_2dlib3foo"),
            "@my-org/my-lib.foo"
        );
    }

    #[test]
    fn demangle_generated_native_symbol_samples() {
        assert_eq!(
            demangle_mangled_function_name("_M0FP29moonbuild20demangle__standalone8demangle"),
            "@moonbuild/demangle_standalone.demangle"
        );
        assert_eq!(
            demangle_mangled_function_name(
                "_M0FP29moonbuild36demangle__standalone__blackbox__test57____test__64656d616e676c655f746573742e6d6274__4_2edyncall$closure.data"
            ),
            "@moonbuild/demangle_standalone_blackbox_test.__test_64656d616e676c655f746573742e6d6274_4.dyncall"
        );
        assert_eq!(
            demangle_mangled_function_name(
                "_M0FP0119moonbitlang_2fcore_2fbuiltin_2fStringBuilder_2eas___40moonbitlang_2fcore_2fbuiltin_2eLogger_2estatic__method__table__id$object.data"
            ),
            "@moonbitlang/core/builtin/StringBuilder.as_@moonbitlang/core/builtin.Logger.static_method_table_id"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0IPB13StringBuilderPB6Logger13write__string"),
            "impl @moonbitlang/core/builtin.Logger for @moonbitlang/core/builtin.StringBuilder with write_string"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0L10local__endS895.$1"),
            "local_end/895"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0L10_2ax__5464S11.$0"),
            "*x_5464/11"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FPB30output_2eflush__segment_7c4024"),
            "@moonbitlang/core/builtin.output.flush_segment|4024"
        );
    }

    #[test]
    fn demangle_additional_edge_cases() {
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp3fooHRP15myapp7MyError"),
            "@myapp.foo raise @myapp.MyError"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0MP04Type3bar"),
            "@Type::bar"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg3foo$closure.data"),
            "@pkg.foo"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg3foo@123"),
            "@pkg.foo"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg3foo."),
            "@pkg.foo"
        );
    }

    #[test]
    fn keeps_original_for_non_or_invalid_mangled_names() {
        assert_eq!(demangle_mangled_function_name("plain"), "plain");
        assert_eq!(
            demangle_mangled_function_name("_M0FP13pkg3foox"),
            "_M0FP13pkg3foox"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0X13pkg3foo"),
            "_M0X13pkg3foo"
        );
        assert_eq!(
            demangle_mangled_function_name("$_M0FP13pkg3foo"),
            "@pkg.foo"
        );
        assert_eq!(
            demangle_mangled_function_name("_M0FP15myapp7try_mapGiE"),
            "_M0FP15myapp7try_mapGiE"
        );
        assert_eq!(
            demangle_mangled_function_name(
                "_M0FP314d_2dh24moonbit_2dscatter_2dplot14scatter_2dplot28gen__scatter__plot__graphics"
            ),
            "_M0FP314d_2dh24moonbit_2dscatter_2dplot14scatter_2dplot28gen__scatter__plot__graphics"
        );
    }
}
