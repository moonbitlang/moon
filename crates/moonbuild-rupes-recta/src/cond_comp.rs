//! Solves conditional compilation directives

use std::path::{Path, PathBuf};

use moonutil::{
    common::TargetBackend,
    cond_expr::{parse_cond_expr, OptLevel, ParseCondExprError, StringOrArray},
    package::MoonPkg,
};

use crate::model;

/// Which kind of test (if any) are we compiling the current package for.
#[derive(Clone, Copy)]
pub enum TestKind {
    Inline,
    Whitebox,
    Blackbox,
}

impl From<model::TargetKind> for Option<TestKind> {
    fn from(value: model::TargetKind) -> Self {
        match value {
            model::TargetKind::Source => None,
            model::TargetKind::WhiteboxTest => Some(TestKind::Whitebox),
            model::TargetKind::BlackboxTest => Some(TestKind::Blackbox),
            model::TargetKind::InlineTest => Some(TestKind::Inline),
            model::TargetKind::SubPackage => None,
        }
    }
}

/// A collection of conditions that affect compilation behavior.
pub struct CompileCondition {
    pub optlevel: OptLevel,
    pub test_kind: Option<TestKind>,
    pub backend: TargetBackend,
}

/// Get the list of files that should get included in the compile list under
/// the given condition.
///
/// Note: `pkg_file_path` is purely for error reporting, as required by
/// [`moonutil::cond_expr::parse_cond_expr`].
pub fn filter_files<'a>(
    pkg: &MoonPkg,
    pkg_file_path: &Path,
    files: impl Iterator<Item = &'a Path>,
    cond: &CompileCondition,
) -> Result<Vec<PathBuf>, ParseCondExprError> {
    let mut res = Vec::new();

    for f in files {
        let filename = f
            .file_name()
            .expect("Input source file should have a filename");
        let str_filename = filename.to_string_lossy();

        let should_include = if let Some(expect_cond) = pkg
            .targets
            .as_ref()
            .and_then(|targets| targets.get(&*str_filename))
        {
            // We have a condition for this file
            should_compile_using_pkg_cond_expr(&str_filename, expect_cond, cond, pkg_file_path)?
        } else {
            // We don't, evaluate file name
            should_compile_using_filename(&str_filename, cond)
        };

        if should_include {
            res.push(f.to_owned());
        }
    }

    Ok(res)
}

fn should_compile_using_pkg_cond_expr(
    name: &str,
    expect: &StringOrArray,
    actual: &CompileCondition,
    pkg_file_path: &Path,
) -> Result<bool, ParseCondExprError> {
    // TODO: Put the parsing earlier, not here
    let cond_expr = parse_cond_expr(pkg_file_path, expect)?;
    if !cond_expr.eval(actual.optlevel, actual.backend) {
        Ok(false) // Fails the condition in pkg.json
    } else if let Some(stripped) = name.strip_suffix(".mbt") {
        Ok(check_test_suffix(stripped, actual.test_kind))
    } else {
        panic!("File name '{}' does not end with '.mbt'", name);
    }
}

fn should_compile_using_filename(name: &str, actual: &CompileCondition) -> bool {
    let Some(filename) = name.strip_suffix(".mbt") else {
        panic!("File name '{}' does not end with '.mbt'", name);
    };

    // Target backend checking -- check the suffix of the file name
    let remaining = match filename.rsplit_once(".") {
        Some((prev, suffix)) => {
            match TargetBackend::str_to_backend(suffix) {
                // correct backend, chop it off
                Ok(backend) if backend == actual.backend => prev,
                Ok(_) => return false, // Wrong backend, returning
                Err(_) => filename,    // No backend suffix, keep the filename as is
            }
        }
        None => filename, // No dot in filename, keep the filename as is
    };

    check_test_suffix(remaining, actual.test_kind)
}

/// Check the suffix of the stripped filename against the actual test condition
fn check_test_suffix(stripped: &str, test_kind: Option<TestKind>) -> bool {
    use FileTestSpec::*;

    // Check test suffixes
    let file_test_spec = if stripped.ends_with("_wbtest") {
        Whitebox
    } else if stripped.ends_with("_test") {
        Blackbox
    } else {
        NoTest
    };
    // White box tests are implemented with compiling with source code, and
    // black box tests are implemented without
    #[allow(clippy::match_like_matches_macro)] // This is more readable
    match (test_kind, file_test_spec) {
        (None, NoTest) => true,
        (Some(TestKind::Whitebox), NoTest | Whitebox) => true,
        (Some(TestKind::Blackbox), Blackbox) => true,
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum FileTestSpec {
    NoTest,
    Whitebox,
    Blackbox,
}
