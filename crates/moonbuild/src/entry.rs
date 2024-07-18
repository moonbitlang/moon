// Copyright 2024 International Digital Economy Academy
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
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use moonutil::module::ModuleDB;
use n2::progress::{DumbConsoleProgress, FancyConsoleProgress, Progress};
use n2::terminal;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

use n2::{trace, work};

use anyhow::{anyhow, Context};
use colored::Colorize;

use crate::check::normal::write_pkg_lst;

use moonutil::common::{is_slash, MoonbuildOpt, MooncOpt, TargetBackend};

use std::sync::{Arc, Mutex};

fn default_parallelism() -> anyhow::Result<usize> {
    let par = std::thread::available_parallelism()?;
    Ok(usize::from(par))
}

#[allow(clippy::type_complexity)]
fn create_progress_console(callback: Option<Box<dyn Fn(&str) + Send>>) -> Box<dyn Progress> {
    if terminal::use_fancy() {
        Box::new(FancyConsoleProgress::new(false, callback))
    } else {
        Box::new(DumbConsoleProgress::new(false, callback))
    }
}

fn render_result(result: Option<usize>, quiet: bool, mode: &str) -> anyhow::Result<i32> {
    match result {
        None => {
            // Don't print any summary, the failing task is enough info.
            anyhow::bail!(format!("failed when {}", mode));
        }
        Some(0) => {
            // Special case: don't print numbers when no work done.
            if !quiet {
                println!("{} moon: no work to do", "Finished.".bright_green().bold());
            }
            Ok(0)
        }
        Some(n) => {
            if !quiet {
                println!(
                    "{} moon: ran {} task{}, now up to date",
                    "Finished.".bright_green().bold(),
                    n,
                    if n == 1 { "" } else { "s" }
                );
            }
            Ok(0)
        }
    }
}

pub fn n2_run_interface(
    state: n2::load::State,
    output_path: PathBuf,
) -> anyhow::Result<Option<usize>> {
    let logger = Arc::new(Mutex::new(vec![]));
    let use_fancy = terminal::use_fancy();

    let catcher = logger.clone();
    let render_and_catch = move |output: &str| {
        output
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                catcher.lock().unwrap().push(content.to_owned());
                moonutil::render::MooncDiagnostic::render(content, use_fancy);
            });
    };

    let mut progress = create_progress_console(Some(Box::new(render_and_catch)));
    let options = work::Options {
        parallelism: default_parallelism()?,
        failures_left: Some(10),
        explain: false,
        adopt: false,
    };
    let mut work = work::Work::new(
        state.graph,
        state.hashes,
        state.db,
        &options,
        progress.as_mut(),
        state.pools,
    );

    if !state.default.is_empty() {
        for target in state.default {
            work.want_file(target)?;
        }
    } else {
        anyhow::bail!("no path specified and no default");
    }

    let res = trace::scope("work.run", || work.run())?;

    if let Some(0) = res {
        // if no work to do, then do not rewrite (build | check | test ...).output
        // instead, read it and print
        let raw_json = std::fs::read_to_string(&output_path)
            .context(format!("failed to open `{}`", output_path.display()))?;

        raw_json
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                moonutil::render::MooncDiagnostic::render(content, use_fancy);
            });
    } else {
        let mut output_file = std::fs::File::create(output_path)?;

        for item in logger.lock().unwrap().iter() {
            output_file.write_all(item.as_bytes())?;
            output_file.write_all("\n".as_bytes())?;
        }
    }

    Ok(res)
}

pub fn run_check(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
) -> anyhow::Result<i32> {
    let state = trace::scope("moonbit::check::read", || {
        crate::check::normal::load_moon_proj(module, moonc_opt, moonbuild_opt)
    })?;

    let result = n2_run_interface(state, moonbuild_opt.target_dir.join("check.output"))?;

    match result {
        Some(0) => {}
        _ => {
            write_pkg_lst(module, &moonbuild_opt.target_dir)?;
        }
    }
    render_result(result, moonbuild_opt.quiet, "checking")
}

pub fn run_build(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
) -> anyhow::Result<i32> {
    let state = trace::scope("moonbit::build::read", || {
        crate::build::load_moon_proj(module, moonc_opt, moonbuild_opt)
    })?;
    let result = n2_run_interface(state, moonbuild_opt.target_dir.join("build.output"))?;
    render_result(result, moonbuild_opt.quiet, "building")
}

pub fn run_run(
    package_path: Option<&String>,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
) -> anyhow::Result<i32> {
    run_build(moonc_opt, moonbuild_opt, module)?;
    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);
    let package_path = package_path
        .unwrap()
        .trim_start_matches("./")
        .trim_start_matches(".\\")
        .trim_end_matches(is_slash);

    let (package_path, last_name): (PathBuf, String) =
        if package_path.is_empty() || package_path == "." {
            let module = moonutil::common::read_module_desc_file_in_dir(source_dir)?;
            let p = std::path::PathBuf::from(module.name);
            (
                PathBuf::from("./"),
                p.file_name().unwrap().to_str().unwrap().into(),
            )
        } else {
            let package_path = std::path::PathBuf::from(package_path);
            let last_name = package_path.file_name().unwrap().to_str().unwrap();
            (package_path.clone(), last_name.into())
        };

    let wat_path = target_dir.join(package_path).join(format!(
        "{}.{}",
        last_name,
        moonc_opt.link_opt.output_format.to_str()
    ));
    let wat_path = dunce::canonicalize(&wat_path)
        .context(format!("cannot find wat file at `{:?}`", &wat_path))?;
    trace::scope("run", || {
        if moonc_opt.link_opt.target_backend == TargetBackend::Wasm
            || moonc_opt.link_opt.target_backend == TargetBackend::WasmGC
        {
            crate::build::run_wat(&wat_path, &moonbuild_opt.args)
        } else {
            crate::build::run_js(&wat_path, &moonbuild_opt.args)
        }
    })?;
    Ok(0)
}

#[derive(Debug, Error)]
pub enum TestFailedStatus {
    #[error("{0}")]
    ApplyExpectFailed(TestResult),

    #[error("{0}")]
    ExpectTestFailed(TestResult),

    #[error("{0}")]
    Failed(TestResult),

    #[error("{0}")]
    RuntimeError(TestResult),

    #[error("{0:?}")]
    Others(#[from] anyhow::Error),
}

impl From<TestFailedStatus> for i32 {
    fn from(value: TestFailedStatus) -> Self {
        match value {
            TestFailedStatus::ApplyExpectFailed(_) => 1,
            TestFailedStatus::ExpectTestFailed(_) => 2,
            TestFailedStatus::Failed(_) => 3,
            TestFailedStatus::RuntimeError(_) => 4,
            TestFailedStatus::Others(_) => 5,
        }
    }
}

#[derive(Debug, Default)]
pub struct TestResult {
    pub passed: u32,
    pub failed: u32,
}

impl std::fmt::Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "passed: {}, failed: {}", self.passed, self.failed)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_test(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    build_only: bool,
    test_verbose_output: bool,
    auto_update: bool,
    module: &ModuleDB,
) -> anyhow::Result<TestResult, TestFailedStatus> {
    let target_dir = &moonbuild_opt.target_dir;
    let state = crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
    let result = n2_run_interface(state, moonbuild_opt.target_dir.join("test.output"))?;
    render_result(result, moonbuild_opt.quiet, "testing")?;

    if build_only {
        return Ok(TestResult::default());
    }

    let state = crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
    let mut defaults: Vec<&String> = state
        .default
        .iter()
        .map(|fid| &state.graph.file(*fid).name)
        .collect();
    if moonbuild_opt.sort_input {
        #[cfg(unix)]
        {
            defaults.sort();
        }
        #[cfg(windows)]
        {
            let normal_slash = defaults
                .iter()
                .enumerate()
                .map(|s| (s.0, s.1.replace('\\', "/")))
                .collect::<Vec<(usize, String)>>();
            let mut new_defaults = defaults.clone();
            for (i, (j, _)) in normal_slash.iter().enumerate() {
                new_defaults[i] = defaults[*j];
            }
            defaults = new_defaults;
        }
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut runtime_error = false;
    let mut expect_failed = false;
    let mut apply_expect_failed = false;

    for d in defaults.iter() {
        let p = Path::new(d);

        match p.extension() {
            Some(name) if name == moonc_opt.link_opt.output_format.to_str() => {
                let result = trace::scope("test", || {
                    if moonc_opt.link_opt.target_backend == TargetBackend::Wasm
                        || moonc_opt.link_opt.target_backend == TargetBackend::WasmGC
                    {
                        crate::runtest::run_wat(p, target_dir)
                    } else {
                        crate::runtest::run_js(p, target_dir)
                    }
                });

                if result.is_err() {
                    let e = result.err().unwrap();
                    eprintln!("Error when running {}: {}", d, e);
                    runtime_error = true;
                } else {
                    let r = result.unwrap();
                    if r.messages
                        .iter()
                        .any(|msg| msg.starts_with(super::expect::EXPECT_FAILED))
                    {
                        expect_failed = true;
                    }
                    if auto_update {
                        if let Err(e) = crate::expect::apply_expect(&r.messages) {
                            eprintln!("{}: {:?}", "failed".red().bold(), e);
                            apply_expect_failed = true;
                        }
                    }
                    passed += r.passed;
                    failed += r.test_names.len() as u32 - r.passed;
                    if test_verbose_output {
                        for i in 0..(r.passed as usize) {
                            println!(
                                "test {}/{}::{} {}",
                                r.package,
                                r.filenames[i],
                                r.test_names[i],
                                "ok".bold().green()
                            )
                        }
                    }

                    for i in 0..(r.test_names.len() - r.passed as usize) {
                        if r.messages[i].starts_with(super::expect::EXPECT_FAILED) {
                            // if we failed at auto update mode, we don't show the below msg to user
                            if !(auto_update && failed > 0 && !apply_expect_failed) {
                                println!(
                                    "test {}/{}::{} {}",
                                    r.package,
                                    r.filenames[i],
                                    r.test_names[i],
                                    "failed".bold().red(),
                                );
                                let _ = crate::expect::render_expect_fail(&r.messages[i]);
                            }
                        } else {
                            println!(
                                "test {}/{}::{} {}: {}",
                                r.package,
                                r.filenames[i],
                                r.test_names[i],
                                "failed".bold().red(),
                                r.messages[i],
                            );
                        }
                    }
                }
            }

            _ => continue,
        }
    }

    let test_result = TestResult { passed, failed };

    if failed == 0 && !runtime_error {
        Ok(test_result)
    } else if apply_expect_failed {
        Err(TestFailedStatus::ApplyExpectFailed(test_result))
    } else if expect_failed {
        Err(TestFailedStatus::ExpectTestFailed(test_result))
    } else if failed != 0 {
        Err(TestFailedStatus::Failed(test_result))
    } else if runtime_error {
        Err(TestFailedStatus::RuntimeError(test_result))
    } else {
        Err(TestFailedStatus::Others(anyhow!("unknown error")))
    }
}

pub fn run_bundle(
    module: &ModuleDB,
    moonbuild_opt: &MoonbuildOpt,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<i32> {
    let state = crate::bundle::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
    let result = n2_run_interface(state, moonbuild_opt.target_dir.join("bundle.output"))?;
    match result {
        Some(0) => {}
        _ => {
            write_pkg_lst(module, &moonbuild_opt.target_dir)?;
        }
    }
    render_result(result, moonbuild_opt.quiet, "bundle")
}

pub fn run_fmt(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<i32> {
    let n2_input = super::fmt::gen_fmt(module, moonc_opt, moonbuild_opt)?;
    let state = super::fmt::gen_n2_fmt_state(&n2_input, moonc_opt, moonbuild_opt)?;
    let _ = n2_run_interface(state, moonbuild_opt.target_dir.join("fmt.output"))?;
    let mut exit_code = 0;
    if moonbuild_opt.fmt_opt.as_ref().unwrap().check {
        for item in n2_input.items.iter() {
            let mut execution = Command::new("git")
                .args([
                    "--no-pager",
                    "diff",
                    "--color=always",
                    "--no-index",
                    &item.input,
                    &item.output,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?;
            let child_stdout = execution.stdout.take().unwrap();
            let mut buf = String::new();
            let mut bufread = std::io::BufReader::new(child_stdout);
            while let Ok(n) = bufread.read_line(&mut buf) {
                if n > 0 {
                    print!("{}", buf);
                    buf.clear()
                } else {
                    break;
                }
            }
            let status = execution.wait()?;
            match status.code() {
                Some(0) => {}
                Some(1) => {
                    exit_code = 1;
                }
                _ => {
                    eprintln!(
                        "failed to execute `git --no-pager diff --color=always --no-index {} {}`",
                        item.input, item.output
                    );
                }
            }
        }
    }
    Ok(exit_code)
}
