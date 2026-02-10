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

use ariadne::ReportKind;
use colored::Colorize;
use n2::progress::{DumbConsoleProgress, FancyConsoleProgress, Progress};
use n2::terminal;
use std::io::Write;

use crate::runtest::TestStatistics;
use moonutil::common::MbtTestInfo;

#[allow(clippy::type_complexity)]
pub fn create_progress_console(
    callback: Option<Box<dyn Fn(&str) + Send>>,
    verbose: bool,
) -> Box<dyn Progress> {
    if terminal::use_fancy() {
        Box::new(FancyConsoleProgress::new(verbose, callback))
    } else {
        Box::new(DumbConsoleProgress::new(verbose, callback))
    }
}

fn render_result(result: &N2RunStats, quiet: bool, mode: &str) -> anyhow::Result<i32> {
    match result.n_tasks_executed {
        None => {
            eprintln!(
                "Failed with {} warnings, {} errors.",
                result.n_warnings, result.n_errors
            );
            anyhow::bail!("failed when {mode} project");
        }
        Some(n_tasks) => {
            if !quiet {
                let finished = "Finished.".green().bold();
                let warnings_errors = format_warnings_errors(result.n_warnings, result.n_errors);

                match n_tasks {
                    0 => {
                        eprintln!("{finished} moon: no work to do{warnings_errors}");
                    }
                    n => {
                        let task_plural = if n == 1 { "" } else { "s" };
                        eprintln!(
                            "{finished} moon: ran {n} task{task_plural}, now up to date{warnings_errors}"
                        );
                    }
                }
            }
        }
    }
    Ok(0)
}

fn format_warnings_errors(n_warnings: usize, n_errors: usize) -> String {
    if n_warnings > 0 || n_errors > 0 {
        format!(" ({n_warnings} warnings, {n_errors} errors)")
    } else {
        String::new()
    }
}

#[derive(Default)]
pub struct ResultCatcher {
    pub content_writer: Vec<String>, // todo: might be better to directly write to string
    pub n_warnings: usize,
    pub n_errors: usize,
}

impl ResultCatcher {
    pub fn append_content(&mut self, s: impl Into<String>, report: Option<ReportKind>) {
        self.content_writer.push(s.into());
        match report {
            Some(ReportKind::Error) => self.n_errors += 1,
            Some(ReportKind::Warning) => self.n_warnings += 1,
            _ => {}
        }
    }

    pub fn append_kind(&mut self, kind: Option<ReportKind>) {
        match kind {
            Some(ReportKind::Error) => self.n_errors += 1,
            Some(ReportKind::Warning) => self.n_warnings += 1,
            _ => {}
        }
    }

    pub fn append_diag(&mut self, diag: &moonutil::render::MooncDiagnostic) {
        if diag.level == "error" {
            self.n_errors += 1;
        } else if diag.level == "warning" {
            self.n_warnings += 1;
        }
    }
}

#[derive(Debug, Clone)]
pub struct N2RunStats {
    /// Number of build tasks executed, `None` means build failure
    pub n_tasks_executed: Option<usize>,

    pub n_errors: usize,
    pub n_warnings: usize,
}

impl N2RunStats {
    /// Whether the run was successful (i.e. didn't fail to execute).
    pub fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }

    /// Get the return code that should be returned to the shell.
    pub fn return_code_for_success(&self) -> i32 {
        if self.successful() { 0 } else { 1 }
    }

    pub fn print_info(&self, quiet: bool, mode: &str) -> anyhow::Result<()> {
        render_result(self, quiet, mode)?;
        Ok(())
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct TestArgs {
    pub package: String,
    pub file_and_index: Vec<(String, Vec<std::ops::Range<u32>>)>,
}

impl TestArgs {
    pub fn to_args(&self) -> String {
        let file_and_index = &self.file_and_index;
        let mut test_params: Vec<(String, u32)> = vec![];
        for (file, ranges) in file_and_index {
            for range in ranges {
                for i in range.clone() {
                    test_params.push((file.clone(), i));
                }
            }
        }
        serde_json::to_string(&test_params).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn to_cli_args_for_native(&self) -> String {
        let mut args = vec![];
        let file_and_index = &self.file_and_index;
        for (file, ranges) in file_and_index {
            for range in ranges {
                args.push(format!("{}:{}-{}", file, range.start, range.end));
            }
        }
        args.join("/")
    }
}

/// Generates compact test output like: `[moontest/lib] my_test.mbt:25 "read should succeed" ok`
///
/// Note: This type was generated by an AI.
pub struct CompactTestFormatter<'a> {
    module_name: &'a str,
    stats: &'a TestStatistics,
    test_info: Option<&'a MbtTestInfo>,
}

impl<'a> CompactTestFormatter<'a> {
    pub fn new(
        module_name: &'a str,
        stats: &'a TestStatistics,
        test_info: Option<&'a MbtTestInfo>,
    ) -> Self {
        Self {
            module_name,
            stats,
            test_info,
        }
    }

    pub fn write_test_identifier<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        if let Some(info) = self.test_info {
            if let Some(name) = info.name.as_ref().filter(|n| !n.is_empty()) {
                write!(w, "{:?}", name)
            } else {
                write!(w, "#{}", info.index)
            }
        } else if !self.stats.test_name.is_empty() {
            write!(w, "{:?}", self.stats.test_name)
        } else {
            write!(w, "#{}", self.stats.index)
        }
    }

    pub fn write_common_prefix<W: Write>(&self, is_bench: bool, w: &mut W) -> std::io::Result<()> {
        // Try to strip the module prefix from the package name for brevity of output
        let stripped = self
            .stats
            .package
            .strip_prefix(self.module_name)
            .map(|x| x.strip_prefix('/').unwrap_or(x));
        // If we have stripped result, this is a local package and we print the module name only
        if stripped.is_some() {
            write!(w, "[{}] ", self.module_name)?;
        } else {
            write!(w, "[{}] ", self.stats.package)?;
        }
        if is_bench {
            write!(w, "bench ")?;
        } else {
            write!(w, "test ")?;
        }
        if let Some(subpackage) = stripped
            && !subpackage.is_empty()
        {
            write!(w, "{}/", subpackage)?;
        }
        write!(w, "{}", self.stats.filename)?;

        let line_number = self.test_info.and_then(|info| info.line_number);
        if let Some(line_num) = line_number {
            write!(w, ":{}", line_num)?;
            write!(w, " (")?;
            self.write_test_identifier(w)?;
            write!(w, ")")
        } else {
            write!(w, " ")?;
            self.write_test_identifier(w)
        }
    }

    pub fn write_success<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.write_common_prefix(false, w)?;
        write!(w, " {}", "ok".green().bold())
    }

    pub fn write_failure<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.write_common_prefix(false, w)?;
        write!(w, " {}", "failed".red().bold())
    }

    pub fn write_failure_with_message<W: Write>(
        &self,
        w: &mut W,
        message: &str,
    ) -> std::io::Result<()> {
        self.write_common_prefix(false, w)?;
        if message.is_empty() {
            write!(w, " {}", "failed".red().bold())
        } else {
            write!(w, " {}: {}", "failed".red().bold(), message)
        }
    }

    pub fn write_bench<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.write_common_prefix(true, w)?;
        write!(w, " {}", "ok".blue())
    }
}
