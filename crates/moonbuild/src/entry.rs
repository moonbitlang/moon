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

use moonutil::module::ModuleDB;
use moonutil::path::PathComponent;
use n2::progress::{DumbConsoleProgress, FancyConsoleProgress, Progress};
use n2::terminal;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use thiserror::Error;

use n2::{trace, work};

use anyhow::Context;
use colored::Colorize;

use crate::check::normal::write_pkg_lst;
use crate::runtest::TestStatistics;

use moonutil::common::{MoonbuildOpt, MooncOpt, TargetBackend};

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
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<Option<usize>> {
    let logger = Arc::new(Mutex::new(vec![]));
    let use_fancy = terminal::use_fancy();

    let catcher = Arc::clone(&logger);
    let output_json = moonbuild_opt.output_json;
    let render_and_catch = move |output: &str| {
        output
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                catcher.lock().unwrap().push(content.to_owned());
                if output_json {
                    println!("{content}");
                } else {
                    moonutil::render::MooncDiagnostic::render(content, use_fancy);
                }
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

    let output_path = moonbuild_opt
        .target_dir
        .join(format!("{}.output", moonbuild_opt.run_mode.to_dir_name()));
    if let Some(0) = res {
        // if no work to do, then do not rewrite (build | check | test ...).output
        // instead, read it and print
        let raw_json = std::fs::read_to_string(&output_path)
            .context(format!("failed to open `{}`", output_path.display()))?;

        raw_json
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                if output_json {
                    println!("{content}");
                } else {
                    moonutil::render::MooncDiagnostic::render(content, use_fancy);
                }
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

    let result = n2_run_interface(state, moonbuild_opt)?;

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
    let result = n2_run_interface(state, moonbuild_opt)?;
    render_result(result, moonbuild_opt.quiet, "building")
}

pub fn run_run(
    package_path: &str,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
) -> anyhow::Result<i32> {
    run_build(moonc_opt, moonbuild_opt, module)?;
    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    let moon_mod = moonutil::common::read_module_desc_file_in_dir(source_dir)?;
    let package_path = {
        let root = if let Some(src) = &moon_mod.source {
            dunce::canonicalize(moonbuild_opt.source_dir.join(src))
                .with_context(|| format!("cannot find root dir `{}`", src))?
        } else {
            dunce::canonicalize(&moonbuild_opt.source_dir).with_context(|| {
                format!(
                    "cannot find root dir `{}`",
                    moonbuild_opt.source_dir.display()
                )
            })?
        };

        let p = dunce::canonicalize(moonbuild_opt.source_dir.join(package_path))
            .with_context(|| format!("cannot find package dir `{}`", package_path))?;

        let rel = p.strip_prefix(&root)?;
        let path_comp = PathComponent::from_path(rel)?;
        path_comp.components.join("/")
    };

    let (package_path, last_name): (PathBuf, String) =
        if package_path.is_empty() || package_path == "." {
            let p = std::path::PathBuf::from(moon_mod.name);
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

#[derive(Debug, Error, Clone)]
pub enum TestFailedStatus {
    #[error("{0}")]
    ApplyExpectFailed(TestStatistics),

    #[error("{0}")]
    ExpectTestFailed(TestStatistics),

    #[error("{0}")]
    Failed(TestStatistics),

    #[error("{0}")]
    RuntimeError(TestStatistics),

    #[error("{0:?}")]
    Others(String),
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
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let target_dir = &moonbuild_opt.target_dir;
    let state = crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;

    let mut runnable_artifacts: Vec<String> = state
        .default
        .iter()
        .map(|fid| state.graph.file(*fid).name.clone())
        .collect();

    let result = n2_run_interface(state, moonbuild_opt)?;
    render_result(result, moonbuild_opt.quiet, "testing")?;

    if build_only {
        return Ok(vec![]);
    }

    if moonbuild_opt.sort_input {
        #[cfg(unix)]
        {
            runnable_artifacts.sort();
        }
        #[cfg(windows)]
        {
            let normal_slash = runnable_artifacts
                .iter()
                .enumerate()
                .map(|s| (s.0, s.1.replace('\\', "/")))
                .collect::<Vec<(usize, String)>>();
            let mut sorted_runnable_artifacts = runnable_artifacts.clone();
            for (i, (j, _)) in normal_slash.iter().enumerate() {
                sorted_runnable_artifacts[i] = runnable_artifacts[*j].clone();
            }
            runnable_artifacts = sorted_runnable_artifacts;
        }
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        // todo: add config item
        .worker_threads(16)
        .enable_all()
        .build()?;

    let mut handlers = vec![];

    let test_opt = &moonbuild_opt.test_opt;
    let filter_package = test_opt.as_ref().and_then(|it| it.filter_package.as_ref());
    let filter_file = test_opt.as_ref().and_then(|it| it.filter_file.as_ref());
    let filter_index = test_opt.as_ref().and_then(|it| it.filter_index);

    let printed = Arc::new(AtomicBool::new(false));
    for (pkgname, _) in module
        .packages
        .iter()
        .filter(|(_, p)| !(p.is_main || p.is_third_party))
    {
        if let Some(package) = filter_package {
            if !package.contains(Path::new(pkgname)) {
                continue;
            }
        }

        let current_pkg_test_info = module.test_info.get(pkgname).unwrap();
        for (artifact_path, map) in current_pkg_test_info {
            if artifact_path.is_none() {
                continue;
            }
            let artifact_path = artifact_path.as_ref().unwrap();

            let wrapper_js_driver_path = artifact_path.with_extension("cjs");
            if moonc_opt.build_opt.target_backend == TargetBackend::Js
                && !wrapper_js_driver_path.exists()
            {
                let js_driver = include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../moonbuild/template/js_driver.js"
                ))
                .replace("origin_js_path", &artifact_path.display().to_string());
                std::fs::write(&wrapper_js_driver_path, js_driver)?;
            }

            for (file_name, test_count) in map {
                if let Some(filter_file) = filter_file {
                    if file_name != filter_file {
                        continue;
                    }
                }

                let range;
                if let Some(filter_index) = filter_index {
                    range = filter_index..(filter_index + 1);
                } else {
                    range = 0..(*test_count);
                }

                let mut args = vec![];
                for index in range.clone() {
                    args.push(pkgname.clone());
                    args.push(file_name.clone());
                    args.push(index.to_string());
                }

                let printed = Arc::clone(&printed);
                handlers.push(async move {
                    let mut result = trace::scope("test", || async {
                        execute_test(
                            moonc_opt.build_opt.target_backend,
                            artifact_path,
                            target_dir,
                            &args,
                        )
                        .await
                    })
                    .await;
                    match result {
                        Ok(ref mut test_res_for_cur_pkg) => {
                            handle_test_result(
                                test_res_for_cur_pkg,
                                moonc_opt,
                                moonbuild_opt,
                                module,
                                auto_update,
                                test_verbose_output,
                                artifact_path,
                                target_dir,
                                printed,
                            )
                            .await?;
                        }
                        Err(_) => {
                            return Ok(vec![
                                Err(
                                    TestFailedStatus::Others("unexpected error".into(),)
                                );
                                range.len()
                            ])
                        }
                    }

                    result
                });
            }
        }
    }

    let res = if moonbuild_opt.no_parallelize {
        runtime.block_on(async {
            let mut results = vec![];
            for handler in handlers {
                results.push(handler.await);
            }
            results
        })
    } else {
        runtime.block_on(futures::future::join_all(handlers))
    };

    let mut r = vec![];
    for item in res {
        // todo: how to handle error for item?
        r.extend(item?.into_iter());
    }

    Ok(r)
}

async fn execute_test(
    target_backend: TargetBackend,
    artifact_path: &Path,
    target_dir: &Path,
    args: &[String],
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    match target_backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => {
            crate::runtest::run_wat(artifact_path, target_dir, args).await
        }
        TargetBackend::Js => {
            crate::runtest::run_js(&artifact_path.with_extension("cjs"), target_dir, args).await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_test_result(
    test_res_for_cur_pkg: &mut Vec<Result<TestStatistics, TestFailedStatus>>,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
    auto_update: bool,
    test_verbose_output: bool,
    artifact_path: &Path,
    target_dir: &Path,
    printed: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    for item in test_res_for_cur_pkg {
        match item {
            Ok(ok_ts) => {
                if test_verbose_output {
                    println!(
                        "test {}/{}::{} {}",
                        ok_ts.package,
                        ok_ts.filename,
                        ok_ts.test_name,
                        "ok".bold().green()
                    );
                }
            }
            Err(TestFailedStatus::RuntimeError(err_ts) | TestFailedStatus::Failed(err_ts)) => {
                println!(
                    "test {}/{}::{} {}: {}",
                    err_ts.package,
                    err_ts.filename,
                    err_ts.test_name,
                    "failed".bold().red(),
                    err_ts.message,
                );
            }
            Err(TestFailedStatus::Others(e)) => {
                eprintln!("{}: {}", "failed".red(), e);
            }
            Err(TestFailedStatus::ExpectTestFailed(origin_err)) => {
                if !auto_update {
                    println!(
                        "test {}/{}::{} {}",
                        origin_err.package,
                        origin_err.filename,
                        origin_err.test_name,
                        "failed".bold().red(),
                    );
                    let _ = crate::expect::render_expect_fail(&origin_err.message);
                }
                if auto_update {
                    if !printed.load(std::sync::atomic::Ordering::SeqCst) {
                        println!(
                            "\n{}\n",
                            "Auto updating expect tests and retesting ...".bold()
                        );
                        printed.store(true, std::sync::atomic::Ordering::SeqCst);
                    }

                    // here need to rerun the test to get the new error message
                    // since the previous apply expect may add or delete some line, which make the error message out of date
                    let rerun = execute_test(
                        moonc_opt.build_opt.target_backend,
                        artifact_path,
                        target_dir,
                        &[
                            origin_err.package.clone(),
                            origin_err.filename.clone(),
                            origin_err.index.clone(),
                        ],
                    )
                    .await?
                    .first()
                    .unwrap()
                    .clone();
                    let update_msg = match rerun {
                        Err(TestFailedStatus::ExpectTestFailed(cur_err)) => &[cur_err.message],
                        _ => &[origin_err.message.clone()],
                    };
                    if let Err(e) = crate::expect::apply_expect(update_msg) {
                        eprintln!("{}: {:?}", "apply expect failed".red().bold(), e);
                    }

                    // recomplie after apply expect
                    {
                        let state =
                            crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
                        n2_run_interface(state, moonbuild_opt)?;
                    }

                    let mut cur_res = execute_test(
                        moonc_opt.build_opt.target_backend,
                        artifact_path,
                        target_dir,
                        &[
                            origin_err.package.clone(),
                            origin_err.filename.clone(),
                            origin_err.index.clone(),
                        ],
                    )
                    .await?
                    .first()
                    .unwrap()
                    .clone();

                    let mut cnt = 1;
                    let limit = moonbuild_opt.test_opt.as_ref().map(|it| it.limit).unwrap();
                    while let Err(TestFailedStatus::ExpectTestFailed(ref etf)) = cur_res {
                        if cnt >= limit {
                            break;
                        }

                        if let Err(e) = crate::expect::apply_expect(&[etf.message.clone()]) {
                            eprintln!("{}: {:?}", "failed".red().bold(), e);
                        }

                        // recomplie after apply expect
                        {
                            let state =
                                crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
                            n2_run_interface(state, moonbuild_opt)?;
                        }

                        cur_res = execute_test(
                            moonc_opt.build_opt.target_backend,
                            artifact_path,
                            target_dir,
                            &[
                                origin_err.package.clone(),
                                origin_err.filename.clone(),
                                origin_err.index.clone(),
                            ],
                        )
                        .await?
                        .first()
                        .unwrap()
                        .clone();

                        cnt += 1;
                    }

                    // update the previous test result
                    *item = cur_res;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn run_bundle(
    module: &ModuleDB,
    moonbuild_opt: &MoonbuildOpt,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<i32> {
    let state = crate::bundle::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
    let result = n2_run_interface(state, moonbuild_opt)?;
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
    let _ = n2_run_interface(state, moonbuild_opt)?;
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
