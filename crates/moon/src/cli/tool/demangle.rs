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

/// Demangle MoonBit symbol names.
#[derive(Debug, clap::Parser)]
pub(crate) struct DemangleSubcommand {
    /// Mangled names to demangle.
    #[clap(value_name = "NAME", required = true)]
    names: Vec<String>,
}

pub(crate) fn run_demangle(cmd: DemangleSubcommand) -> anyhow::Result<i32> {
    for name in cmd.names {
        println!("{}", demangle_mangled_function_name(&name));
    }
    Ok(0)
}

pub(crate) fn demangle_mangled_function_name(func_name: &str) -> String {
    demangle_mangled_function_name_impl(func_name).unwrap_or_else(|| func_name.to_string())
}

fn demangle_mangled_function_name_impl(func_name: &str) -> Option<String> {
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

    let (text, j) = match tag {
        b'F' => demangle_tag_f(func_name, i),
        b'M' => demangle_tag_m(func_name, i),
        b'I' => demangle_tag_i(func_name, i),
        b'E' => demangle_tag_e(func_name, i),
        b'T' => demangle_tag_t(func_name, i),
        b'L' => demangle_tag_l(func_name, i),
        _ => None,
    }?;

    if j < func_name.len() {
        match byte_at(func_name, j) {
            Some(b'.' | b'$' | b'@') => {}
            _ => return None,
        }
    }
    Some(text)
}

fn demangle_tag_f(s: &str, i: usize) -> Option<(String, usize)> {
    let (pkg, pkg_end) = parse_package(s, i)?;
    let (name, mut j) = parse_identifier(s, pkg_end)?;
    let mut text = if pkg.is_empty() {
        format!("@{name}")
    } else {
        format!("@{pkg}.{name}")
    };

    while byte_at(s, j) == Some(b'N') {
        let (nested, nested_end) = parse_identifier(s, j + 1)?;
        text.push('.');
        text.push_str(&nested);
        j = nested_end;
    }

    if byte_at(s, j) == Some(b'C') {
        j += 1;
        let start = j;
        while byte_at(s, j).is_some_and(is_digit) {
            j += 1;
        }
        if start == j {
            return None;
        }
        let idx = &s[start..j];
        text.push_str(&format!(".{idx} (the {idx}-th anonymous-function)"));
    }

    if matches!(byte_at(s, j), Some(b'G' | b'H')) {
        let (args, args_end) = parse_type_args(s, j)?;
        text.push_str(&args);
        j = args_end;
    }

    Some((text, j))
}

fn demangle_tag_m(s: &str, i: usize) -> Option<(String, usize)> {
    let (pkg, pkg_end) = parse_package(s, i)?;
    let (type_name, type_end) = parse_identifier(s, pkg_end)?;
    let (method, method_end) = parse_identifier(s, type_end)?;

    let mut text = if pkg.is_empty() {
        format!("@{type_name}::{method}")
    } else {
        format!("@{pkg}.{type_name}::{method}")
    };
    let mut j = method_end;

    if matches!(byte_at(s, j), Some(b'G' | b'H')) {
        let (args, args_end) = parse_type_args(s, j)?;
        text.push_str(&args);
        j = args_end;
    }

    Some((text, j))
}

fn demangle_tag_i(s: &str, i: usize) -> Option<(String, usize)> {
    let (impl_type, impl_end) = append_type_path(s, i, false)?;
    let (trait_type, trait_end) = append_type_path(s, impl_end, false)?;
    let (method, method_end) = parse_identifier(s, trait_end)?;

    let mut j = method_end;
    let mut type_args = String::new();
    if matches!(byte_at(s, j), Some(b'G' | b'H')) {
        let (args, args_end) = parse_type_args(s, j)?;
        type_args = args;
        j = args_end;
    }

    let text = format!("impl {trait_type} for {impl_type}{type_args} with {method}");
    Some((text, j))
}

fn demangle_tag_e(s: &str, i: usize) -> Option<(String, usize)> {
    let (type_pkg, type_pkg_end) = parse_package(s, i)?;
    let (type_name, type_name_end) = parse_identifier(s, type_pkg_end)?;
    let (method_pkg, method_pkg_end) = parse_package(s, type_name_end)?;
    let (method_name, method_name_end) = parse_identifier(s, method_pkg_end)?;

    let type_pkg_use = if is_core_package(&type_pkg) {
        ""
    } else {
        type_pkg.as_str()
    };

    let mut text = String::from("@");
    if !method_pkg.is_empty() {
        text.push_str(&method_pkg);
        text.push('.');
    }
    if !type_pkg_use.is_empty() {
        text.push_str(type_pkg_use);
        text.push('.');
    }
    text.push_str(&type_name);
    text.push_str("::");
    text.push_str(&method_name);

    let mut j = method_name_end;
    if matches!(byte_at(s, j), Some(b'G' | b'H')) {
        let (args, args_end) = parse_type_args(s, j)?;
        text.push_str(&args);
        j = args_end;
    }

    Some((text, j))
}

fn demangle_tag_t(s: &str, i: usize) -> Option<(String, usize)> {
    append_type_path(s, i, false)
}

fn demangle_tag_l(s: &str, i: usize) -> Option<(String, usize)> {
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
    let stamp = &s[stamp_start..j];

    let no_dollar = ident.strip_prefix('$').unwrap_or(&ident);
    let text = format!("{}/{}", strip_suffix(no_dollar, ".fn"), stamp);
    Some((text, j))
}

fn append_type_path(s: &str, i: usize, omit_core_prefix: bool) -> Option<(String, usize)> {
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

    let out = if pkg.is_empty() {
        format!("@{type_name}")
    } else {
        format!("@{pkg}.{type_name}")
    };
    Some((out, k))
}

fn parse_type_ref(s: &str, i: usize) -> Option<(String, usize)> {
    if byte_at(s, i) != Some(b'R') {
        return None;
    }
    let (mut text, mut j) = append_type_path(s, i + 1, false)?;
    if byte_at(s, j) == Some(b'G') {
        let (args, args_end) = parse_type_args(s, j)?;
        text.push_str(&args);
        j = args_end;
    }
    Some((text, j))
}

fn parse_fn_type(s: &str, i: usize, async_mark: bool) -> Option<(String, usize)> {
    if byte_at(s, i) != Some(b'W') {
        return None;
    }

    let mut j = i + 1;
    let mut params = Vec::new();
    while byte_at(s, j) != Some(b'E') {
        let (param, param_end) = parse_type_arg(s, j)?;
        params.push(param);
        j = param_end;
    }
    j += 1;

    let (ret, ret_end) = parse_type_arg(s, j)?;
    j = ret_end;

    let mut raises = String::new();
    if byte_at(s, j) == Some(b'Q') {
        let (raised, raised_end) = parse_type_arg(s, j + 1)?;
        raises = format!(" raise {raised}");
        j = raised_end;
    }

    let prefix = if async_mark { "async " } else { "" };
    Some((
        format!("{prefix}({}) -> {ret}{raises}", params.join(", ")),
        j,
    ))
}

fn parse_type_args(s: &str, i: usize) -> Option<(String, usize)> {
    if byte_at(s, i) == Some(b'H') {
        let (raised, raised_end) = parse_type_arg(s, i + 1)?;
        return Some((format!(" raise {raised}"), raised_end));
    }

    if byte_at(s, i) != Some(b'G') {
        return Some((String::new(), i));
    }

    let mut j = i + 1;
    let mut args = Vec::new();
    while byte_at(s, j) != Some(b'E') {
        let (arg, arg_end) = parse_type_arg(s, j)?;
        args.push(arg);
        j = arg_end;
    }
    j += 1;

    let mut suffix = String::new();
    if byte_at(s, j) == Some(b'H') {
        let (raised, raised_end) = parse_type_arg(s, j + 1)?;
        suffix = format!(" raise {raised}");
        j = raised_end;
    }

    Some((format!("[{}]{suffix}", args.join(", ")), j))
}

fn parse_type_arg(s: &str, i: usize) -> Option<(String, usize)> {
    let c = byte_at(s, i)?;
    match c {
        b'i' => Some(("Int".to_string(), i + 1)),
        b'l' => Some(("Int64".to_string(), i + 1)),
        b'h' => Some(("Int16".to_string(), i + 1)),
        b'j' => Some(("UInt".to_string(), i + 1)),
        b'k' => Some(("UInt16".to_string(), i + 1)),
        b'm' => Some(("UInt64".to_string(), i + 1)),
        b'd' => Some(("Double".to_string(), i + 1)),
        b'f' => Some(("Float".to_string(), i + 1)),
        b'b' => Some(("Bool".to_string(), i + 1)),
        b'c' => Some(("Char".to_string(), i + 1)),
        b's' => Some(("String".to_string(), i + 1)),
        b'u' => Some(("Unit".to_string(), i + 1)),
        b'y' => Some(("Byte".to_string(), i + 1)),
        b'z' => Some(("Bytes".to_string(), i + 1)),
        b'A' => {
            let (inner, inner_end) = parse_type_arg(s, i + 1)?;
            Some((format!("FixedArray[{inner}]"), inner_end))
        }
        b'O' => {
            let (inner, inner_end) = parse_type_arg(s, i + 1)?;
            Some((format!("Option[{inner}]"), inner_end))
        }
        b'U' => {
            let mut j = i + 1;
            let mut elems = Vec::new();
            while byte_at(s, j) != Some(b'E') {
                let (elem, elem_end) = parse_type_arg(s, j)?;
                elems.push(elem);
                j = elem_end;
            }
            Some((format!("({})", elems.join(", ")), j + 1))
        }
        b'V' => parse_fn_type(s, i + 1, true),
        b'W' => parse_fn_type(s, i, false),
        b'R' => parse_type_ref(s, i),
        _ => None,
    }
}

fn parse_package(s: &str, mut i: usize) -> Option<(String, usize)> {
    if byte_at(s, i) != Some(b'P') {
        return None;
    }
    i += 1;

    let count_start = i;
    let (mut count, j) = parse_u32(s, i)?;
    if let Some(pkg) = parse_package_segments(s, j, count) {
        return Some(pkg);
    }

    // Backward-compatible fallback: single-digit package segment count.
    i = count_start;
    let digit = byte_at(s, i)?;
    if !is_digit(digit) {
        return None;
    }
    count = (digit - b'0') as u32;
    i += 1;
    parse_package_segments(s, i, count)
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
            demangle_mangled_function_name("_M0IP05outerL5LocalP311moonbitlang4core7builtin7Default7defaultGiE"),
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
            demangle_mangled_function_name("_M0FP15myapp7complexGARP311moonbitlang4core4list4ListGiEE"),
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
    }
}
