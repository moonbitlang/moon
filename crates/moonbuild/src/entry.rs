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
use log::warn;
use moonutil::module::ModuleDB;
use moonutil::package::Package;
use moonutil::path::PathComponent;
use n2::graph::FileId;
use n2::load::State;
use n2::progress::{DumbConsoleProgress, FancyConsoleProgress, Progress};
use n2::terminal;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use thiserror::Error;

use n2::{trace, work};

use anyhow::Context;
use colored::Colorize;

use crate::benchmark::{render_batch_bench_summary, BATCHBENCH};
use crate::check::normal::write_pkg_lst;
use crate::expect::{apply_snapshot, render_snapshot_fail};
use crate::runtest::TestStatistics;

use moonutil::common::{
    DriverKind, FileLock, FileName, MoonbuildOpt, MooncGenTestInfo, MooncOpt, PrePostBuild,
    TargetBackend, TestArtifacts, TestBlockIndex, TestName, DOT_MBT_DOT_MD, TEST_INFO_FILE,
};

use std::sync::{Arc, Mutex};

fn default_parallelism() -> anyhow::Result<usize> {
    let par = std::thread::available_parallelism()?;
    Ok(usize::from(par))
}

#[allow(clippy::type_complexity)]
fn create_progress_console(
    callback: Option<Box<dyn Fn(&str) + Send>>,
    verbose: bool,
) -> Box<dyn Progress> {
    if terminal::use_fancy() {
        Box::new(FancyConsoleProgress::new(verbose, callback))
    } else {
        Box::new(DumbConsoleProgress::new(verbose, callback))
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
                eprintln!("{} moon: no work to do", "Finished.".bright_green().bold());
            }
            Ok(0)
        }
        Some(n) => {
            if !quiet {
                eprintln!(
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

pub fn n2_simple_run_interface(
    state: n2::load::State,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<Option<usize>> {
    let logger = Arc::new(Mutex::new(vec![]));
    let use_fancy = terminal::use_fancy();

    let catcher = Arc::clone(&logger);
    let output_json = moonbuild_opt.output_json;
    let check_patch_file = moonbuild_opt
        .check_opt
        .as_ref()
        .and_then(|it| it.patch_file.clone());
    let explain = moonbuild_opt
        .check_opt
        .as_ref()
        .is_some_and(|it| it.explain);

    let (target_dir, source_dir) = (
        moonbuild_opt.target_dir.clone(),
        moonbuild_opt.source_dir.clone(),
    );
    let render_and_catch = move |output: &str| {
        output
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                catcher.lock().unwrap().push(content.to_owned());
                if output_json {
                    println!("{content}");
                } else {
                    moonutil::render::MooncDiagnostic::render(
                        content,
                        use_fancy,
                        check_patch_file.clone(),
                        explain,
                        (target_dir.clone(), source_dir.clone()),
                    );
                }
            });
    };

    // TODO: generate build graph for pre_build?

    let mut progress =
        create_progress_console(Some(Box::new(render_and_catch)), moonbuild_opt.verbose);
    let options = work::Options {
        parallelism: get_parallelism(moonbuild_opt)?,
        failures_left: Some(10),
        explain: false,
        adopt: false,
        dirty_on_output: true,
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
        return Ok(Some(0));
    }

    let res = trace::scope("work.run", || work.run())?;
    Ok(res)
}

pub fn get_parallelism(opt: &MoonbuildOpt) -> anyhow::Result<usize> {
    if let Ok(val) = std::env::var("MOON_MAX_PAR_TASKS") {
        val.parse()
            .context("Failed to parse MOON_MAX_PAR_TASKS to get the parallelism for building")
    } else if let Some(par) = opt.parallelism {
        Ok(par)
    } else if let Ok(val) = default_parallelism() {
        Ok(val)
    } else {
        warn!("Failed to get the parallelism for building, falling back to 1 parallel task");
        Ok(1)
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
    let check_patch_file = moonbuild_opt
        .check_opt
        .as_ref()
        .and_then(|it| it.patch_file.clone());
    let explain = moonbuild_opt
        .check_opt
        .as_ref()
        .is_some_and(|it| it.explain);

    let (target_dir, source_dir) = (
        moonbuild_opt.target_dir.clone(),
        moonbuild_opt.source_dir.clone(),
    );
    let render_and_catch = move |output: &str| {
        output.lines().for_each(|content| {
            catcher.lock().unwrap().push(content.to_owned());
            if output_json {
                println!("{content}");
            } else {
                moonutil::render::MooncDiagnostic::render(
                    content,
                    use_fancy,
                    check_patch_file.clone(),
                    explain,
                    (target_dir.clone(), source_dir.clone()),
                );
            }
        });
    };

    if moonbuild_opt.build_graph {
        vis_build_graph(&state, moonbuild_opt);
    }

    let mut progress =
        create_progress_console(Some(Box::new(render_and_catch)), moonbuild_opt.verbose);
    let options = work::Options {
        parallelism: get_parallelism(moonbuild_opt)?,
        failures_left: Some(10),
        explain: false,
        adopt: false,
        dirty_on_output: true,
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
    let (target_dir, source_dir) = (
        moonbuild_opt.target_dir.clone(),
        moonbuild_opt.source_dir.clone(),
    );
    if let Some(0) = res {
        // if no work to do, then do not rewrite (build | check | test ...).output
        // instead, read it and print
        let raw_json = std::fs::read_to_string(&output_path)
            .context(format!("failed to open `{}`", output_path.display()))?;

        let check_patch_file = moonbuild_opt
            .check_opt
            .as_ref()
            .and_then(|it| it.patch_file.clone());
        raw_json.lines().for_each(|content| {
            if output_json {
                println!("{content}");
            } else {
                moonutil::render::MooncDiagnostic::render(
                    content,
                    use_fancy,
                    check_patch_file.clone(),
                    moonbuild_opt
                        .check_opt
                        .as_ref()
                        .is_some_and(|it| it.explain),
                    (target_dir.clone(), source_dir.clone()),
                );
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

fn vis_build_graph(state: &State, moonbuild_opt: &MoonbuildOpt) {
    let path = moonbuild_opt.target_dir.join("build_graph.dot");
    let source_dir = moonbuild_opt.source_dir.display().to_string();

    let graph = &state.graph;
    let files = &graph.files;
    let builds = &graph.builds;
    let default_artifact = state
        .default
        .clone()
        .into_iter()
        .collect::<HashSet<FileId>>();

    let mut dot = String::from("digraph BuildGraph {\n");

    for file_id in files.all_ids() {
        let file_name = &files.by_id[file_id].name.replace(&source_dir, ".");
        // mark the file if it's the default artifact that we really want
        let (style, fontcolor) = if default_artifact.contains(&file_id) {
            ("style=filled, fillcolor=black", "fontcolor=white")
        } else {
            ("color=black", "")
        };
        dot.push_str(&format!(
            "    \"{file_name}\" [shape=box, {style}, {fontcolor}];\n"
        ));
    }

    let default_desc = "missing description".to_string();
    for build in builds.iter() {
        let build_desc = build
            .desc
            .as_ref()
            .unwrap_or(&default_desc)
            .replace(&source_dir, ".");
        dot.push_str(&format!("    \"{build_desc}\" [shape=ellipse];\n"));

        for &input_id in build.ins.ids.iter() {
            let input_file_name = &files.by_id[input_id].name.replace(&source_dir, ".");
            dot.push_str(&format!("    \"{input_file_name}\" -> \"{build_desc}\";\n"));
        }

        for &output_id in build.outs() {
            let output_file_name = &files.by_id[output_id].name.replace(&source_dir, ".");
            dot.push_str(&format!(
                "    \"{build_desc}\" -> \"{output_file_name}\";\n"
            ));
        }
    }

    dot.push_str("}\n");
    std::fs::write(&path, dot).expect("Unable to write dot file");
    eprintln!("generated build graph: {}", path.display());
}

#[derive(Copy, Clone)]
pub enum MoonXBuildState {
    NoWork,
    WorkDone,
}

pub fn run_moon_x_build(
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
    build_type: &PrePostBuild,
) -> anyhow::Result<MoonXBuildState> {
    let common = moonbuild_opt.raw_target_dir.join("common");
    if !common.exists() {
        std::fs::create_dir_all(&common)?;
    }
    let _lock = FileLock::lock(&common)?;

    let x_build_state = crate::pre_build::load_moon_x_build(moonbuild_opt, module, build_type)?;
    if let Some(x_build_state) = x_build_state {
        let pre_build_result = n2_simple_run_interface(x_build_state, moonbuild_opt)?;
        render_x_build_result(pre_build_result, moonbuild_opt.quiet, build_type)?;
        Ok(MoonXBuildState::WorkDone)
    } else {
        Ok(MoonXBuildState::NoWork)
    }
}

fn render_x_build_result(
    result: Option<usize>,
    quiet: bool,
    build_type: &PrePostBuild,
) -> anyhow::Result<i32> {
    match result {
        None => {
            anyhow::bail!(format!("failed when execute {} task(s)", build_type.name()));
        }
        Some(0) => Ok(0),
        Some(n) => {
            if !quiet {
                eprintln!(
                    "Executed {} {} task{}, now up to date",
                    n,
                    build_type.name(),
                    if n == 1 { "" } else { "s" }
                );
            }
            Ok(0)
        }
    }
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

    write_pkg_lst(module, &moonbuild_opt.raw_target_dir)?;
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
    build_only: bool,
) -> anyhow::Result<i32> {
    run_build(moonc_opt, moonbuild_opt, module)?;
    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    let moon_mod = moonutil::common::read_module_desc_file_in_dir(source_dir)?;
    let package_path = {
        let root = if let Some(src) = &moon_mod.source {
            dunce::canonicalize(moonbuild_opt.source_dir.join(src))
                .with_context(|| format!("cannot find root dir `{src}`"))?
        } else {
            dunce::canonicalize(&moonbuild_opt.source_dir).with_context(|| {
                format!(
                    "cannot find root dir `{}`",
                    moonbuild_opt.source_dir.display()
                )
            })?
        };

        let p = dunce::canonicalize(moonbuild_opt.source_dir.join(package_path))
            .with_context(|| format!("cannot find package dir `{package_path}`"))?;

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

    if build_only {
        let test_artifacts = TestArtifacts {
            artifacts_path: vec![wat_path],
        };
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        return Ok(0);
    }

    trace::scope("run", || match moonc_opt.link_opt.target_backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => {
            crate::build::run_wat(&wat_path, &moonbuild_opt.args, moonbuild_opt.verbose)
        }
        TargetBackend::Js => {
            crate::build::run_js(&wat_path, &moonbuild_opt.args, moonbuild_opt.verbose)
        }
        TargetBackend::Native | TargetBackend::LLVM => crate::build::run_native(
            &wat_path.with_extension("exe"),
            &moonbuild_opt.args,
            moonbuild_opt.verbose,
        ),
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

    #[error("{0}")]
    SnapshotPending(TestStatistics),

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
            TestFailedStatus::SnapshotPending(_) => 5,
            TestFailedStatus::Others(_) => 6,
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

pub type FileTestInfo = IndexMap<FileName, IndexMap<TestBlockIndex, Option<TestName>>>;
fn convert_moonc_test_info(
    test_info_file: &Path,
    pkg: &Package,
    output_format: &str,
    filter_file: Option<&String>,
    sort_input: bool,
) -> anyhow::Result<IndexMap<(PathBuf, DriverKind), FileTestInfo>> {
    let mut test_info_files = vec![];
    for driver_kind in [
        DriverKind::Internal,
        DriverKind::Whitebox,
        DriverKind::Blackbox,
    ] {
        let path = test_info_file.join(format!("__{driver_kind}_{TEST_INFO_FILE}"));
        if path.exists() {
            test_info_files.push((driver_kind, path));
        }
    }

    let mut current_pkg_test_info = IndexMap::new();
    for (driver_kind, test_info_file) in test_info_files {
        let content = std::fs::read_to_string(&test_info_file)
            .context(format!("failed to read {}", test_info_file.display()))?;

        let info = serde_json_lenient::from_str::<MooncGenTestInfo>(&content)
            .context(format!("failed to parse {}", test_info_file.display()))?;

        let artifact_path = pkg
            .artifact
            .with_file_name(format!("{}.{}_test.wat", pkg.last_name(), driver_kind))
            .with_extension(output_format);

        for (filename, test_info) in info
            .no_args_tests
            .into_iter()
            .chain(info.with_args_tests.into_iter())
            .chain(info.with_bench_args_tests.into_iter())
        {
            if test_info.is_empty() {
                continue;
            }
            if let Some(filter_file) = filter_file {
                if filename != *filter_file {
                    continue;
                }
            }
            current_pkg_test_info
                .entry((artifact_path.clone(), driver_kind))
                .or_insert(IndexMap::new())
                .entry(filename)
                .or_insert(IndexMap::new())
                .extend(test_info.iter().map(|it| (it.index, it.name.clone())));
        }
    }

    if sort_input {
        current_pkg_test_info.sort_keys();
    }

    Ok(current_pkg_test_info)
}

#[allow(clippy::too_many_arguments)]
pub fn run_test(
    moonc_opt: MooncOpt,
    moonbuild_opt: MoonbuildOpt,
    build_only: bool,
    test_verbose_output: bool,
    auto_update: bool,
    module: ModuleDB,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let moonc_opt = Arc::new(moonc_opt);
    let moonbuild_opt = Arc::new(moonbuild_opt);
    let module = Arc::new(module);

    let state = crate::runtest::load_moon_proj(&module, &moonc_opt, &moonbuild_opt)?;
    let result = n2_run_interface(state, &moonbuild_opt)?;
    render_result(result, moonbuild_opt.quiet, "testing")?;

    let mut handlers = vec![];

    let test_opt = &moonbuild_opt.test_opt;
    let filter_package = test_opt.as_ref().and_then(|it| it.filter_package.as_ref());
    let filter_file = test_opt.as_ref().and_then(|it| it.filter_file.as_ref());
    let filter_index = test_opt.as_ref().and_then(|it| it.filter_index);
    let filter_doc_index = test_opt.as_ref().and_then(|it| it.filter_doc_index);

    let printed = Arc::new(AtomicBool::new(false));
    let mut test_artifacts = TestArtifacts {
        artifacts_path: vec![],
    };
    for (pkgname, pkg) in module
        .get_all_packages()
        .iter()
        .filter(|(_, p)| !p.is_third_party)
    {
        if let Some(package) = filter_package {
            if !package.contains(pkgname) {
                continue;
            }
        }

        // convert moonc test info
        let test_info_file_dir = moonbuild_opt.target_dir.join(pkg.rel.fs_full_name());
        let current_pkg_test_info = convert_moonc_test_info(
            &test_info_file_dir,
            pkg,
            moonc_opt.link_opt.target_backend.to_extension(),
            filter_file,
            moonbuild_opt.sort_input,
        )?;

        for ((artifact_path, driver_kind), file_test_info_map) in current_pkg_test_info {
            match (driver_kind, filter_file, filter_doc_index, filter_index) {
                // internal test can't be filtered by --doc-index
                (DriverKind::Internal, _, Some(_), _) => {
                    continue;
                }
                // blackbox test only valid for _test.mbt and .mbt.md
                (DriverKind::Blackbox, Some(filename), _, Some(_))
                    if !filename.ends_with("_test.mbt") && !filename.ends_with(DOT_MBT_DOT_MD) =>
                {
                    continue;
                }
                // blackbox test in _test.mbt or .mbt.md can't run doc test
                (DriverKind::Blackbox, Some(filename), Some(_), _)
                    if filename.ends_with("_test.mbt") || filename.ends_with(DOT_MBT_DOT_MD) =>
                {
                    continue;
                }
                _ => {}
            }
            let mut test_args = TestArgs {
                package: pkgname.clone(),
                file_and_index: vec![],
            };
            for (file_name, test_count) in &file_test_info_map {
                let range;
                let filter_index = filter_index.or(filter_doc_index);
                if let Some(filter_index) = filter_index {
                    range = filter_index..(filter_index + 1);
                } else {
                    range = 0..(test_count.len() as u32);
                }
                test_args.file_and_index.push((file_name.clone(), range));
            }

            let wrapper_js_driver_path = artifact_path.with_extension("cjs");
            if moonc_opt.build_opt.target_backend == TargetBackend::Js {
                let js_driver = include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../moonbuild/template/test_driver/js_driver.js"
                ))
                .replace(
                    "origin_js_path",
                    &artifact_path.display().to_string().replace("\\", "/"),
                )
                .replace(
                    "let testParams = []",
                    &format!("let testParams = {}", test_args.to_args()),
                )
                .replace(
                    "let packageName = \"\"",
                    &format!("let packageName = {:?}", test_args.package),
                );

                std::fs::write(&wrapper_js_driver_path, js_driver)?;
                // prevent node use the outer layer package.json with `"type": "module"`
                std::fs::write(moonbuild_opt.target_dir.join("package.json"), "{}")?;
                test_artifacts
                    .artifacts_path
                    .push(wrapper_js_driver_path.clone());
            } else {
                test_artifacts.artifacts_path.push(artifact_path.clone());
            }

            let printed = Arc::clone(&printed);
            let moonc_opt = Arc::clone(&moonc_opt);
            let moonbuild_opt = Arc::clone(&moonbuild_opt);
            let module = Arc::clone(&module);
            handlers.push(async move {
                let mut result = trace::async_scope(
                    "test",
                    execute_test(
                        &moonbuild_opt,
                        moonc_opt.build_opt.target_backend,
                        &artifact_path,
                        &moonbuild_opt.target_dir,
                        &test_args,
                        &file_test_info_map,
                    ),
                )
                .await;
                match result {
                    Ok(ref mut test_res_for_cur_pkg) => {
                        handle_test_result(
                            test_res_for_cur_pkg,
                            &moonc_opt,
                            &moonbuild_opt,
                            &module,
                            auto_update,
                            test_verbose_output,
                            &artifact_path,
                            &moonbuild_opt.target_dir,
                            printed,
                            &file_test_info_map,
                        )
                        .await?;
                    }
                    Err(e) => {
                        eprintln!("{:?}\n", &e);
                        // when spawn process failed, this can still make the total test count to be correct
                        // but this is not a good way to handle it
                        return Ok(vec![
                            Err(TestFailedStatus::Others(e.to_string()));
                            test_args.get_test_cnt() as usize
                        ]);
                    }
                }

                result
            });
        }
    }

    if build_only {
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        return Ok(vec![]);
    }

    let res = if moonbuild_opt.no_parallelize {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        runtime.block_on(async {
            let mut results = vec![];
            for handler in handlers {
                // Tasks are run sequentially by using the `await` expression directly.
                results.push(handler.await);
            }
            results
        })
    } else {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        runtime.block_on(async {
            let mut res_handlers = vec![];
            for handler in handlers {
                // Submit tasks to the scheduler
                res_handlers.push(runtime.spawn(handler));
            }
            futures::future::join_all(res_handlers)
                .await
                .into_iter()
                .map(|res| res.unwrap())
                .collect()
        })
    };

    let mut r = vec![];
    for item in res {
        // todo: how to handle error for item?
        r.extend(item?.into_iter());
    }

    Ok(r)
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct TestArgs {
    pub package: String,
    pub file_and_index: Vec<(String, std::ops::Range<u32>)>,
}

impl TestArgs {
    fn get_test_cnt(&self) -> u32 {
        self.file_and_index
            .iter()
            .map(|(_, range)| range.end - range.start)
            .sum()
    }

    fn to_args(&self) -> String {
        let file_and_index = &self.file_and_index;
        let mut test_params: Vec<[String; 2]> = vec![];
        for (file, index) in file_and_index {
            for i in index.clone() {
                test_params.push([file.clone(), i.to_string()]);
            }
        }
        format!("{test_params:?}")
    }

    pub fn to_cli_args_for_native(&self) -> String {
        let mut args = vec![];
        let file_and_index = &self.file_and_index;
        for (file, index) in file_and_index {
            args.push(format!("{}:{}-{}", file, index.start, index.end));
        }
        args.join("/")
    }
}

async fn execute_test(
    moonbuild_opt: &MoonbuildOpt,
    target_backend: TargetBackend,
    artifact_path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let verbose = moonbuild_opt.verbose;
    match target_backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => {
            crate::runtest::run_wat(artifact_path, target_dir, args, file_test_info_map, verbose)
                .await
        }
        TargetBackend::Js => {
            crate::runtest::run_js(
                &artifact_path.with_extension("cjs"),
                target_dir,
                args,
                file_test_info_map,
                verbose,
            )
            .await
        }
        TargetBackend::Native => {
            crate::runtest::run_native(
                moonbuild_opt,
                artifact_path,
                target_dir,
                args,
                file_test_info_map,
                verbose,
            )
            .await
        }
        TargetBackend::LLVM => {
            crate::runtest::run_llvm(artifact_path, target_dir, args, file_test_info_map, verbose)
                .await
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
    file_test_info_map: &FileTestInfo,
) -> anyhow::Result<()> {
    let output_failure_in_json = moonbuild_opt
        .test_opt
        .as_ref()
        .map(|it| it.test_failure_json)
        .unwrap_or(false);
    for item in test_res_for_cur_pkg {
        match item {
            Ok(ok_ts) => {
                if ok_ts.message.starts_with(BATCHBENCH) {
                    let stat = ok_ts;
                    println!(
                        "bench {}/{}::{}",
                        stat.package, stat.filename, stat.test_name,
                    );
                    render_batch_bench_summary(&stat.message);
                } else if test_verbose_output {
                    println!(
                        "test {}/{}::{} {}",
                        ok_ts.package,
                        ok_ts.filename,
                        ok_ts.test_name,
                        "ok".bold().green()
                    );
                }
            }
            Err(TestFailedStatus::SnapshotPending(stat)) => {
                if !auto_update {
                    if output_failure_in_json {
                        println!("{}", serde_json_lenient::to_string(stat)?);
                    } else {
                        println!(
                            "test {}/{}::{} {}",
                            stat.package,
                            stat.filename,
                            stat.test_name,
                            "failed".bold().red(),
                        );
                    }
                    let _ = render_snapshot_fail(&stat.message);
                }
                if auto_update {
                    if !printed.load(std::sync::atomic::Ordering::SeqCst) {
                        println!(
                            "\n{}\n",
                            "Auto updating expect tests and retesting ...".bold()
                        );
                        printed.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    apply_snapshot(&[stat.message.to_string()])?;
                    let index = stat.index.clone().parse::<u32>().unwrap();
                    let test_args = TestArgs {
                        package: stat.package.clone(),
                        file_and_index: vec![(stat.filename.clone(), index..(index + 1))],
                    };
                    let rerun = execute_test(
                        moonbuild_opt,
                        moonc_opt.build_opt.target_backend,
                        artifact_path,
                        target_dir,
                        &test_args,
                        file_test_info_map,
                    )
                    .await?
                    .first()
                    .unwrap()
                    .clone();

                    let update_msg = match rerun {
                        // if rerun test success, update the previous test result and continue
                        Ok(_) => {
                            *item = rerun;
                            continue;
                        }
                        Err(TestFailedStatus::SnapshotPending(cur_err)) => &[cur_err.message],
                        _ => &[stat.message.clone()],
                    };
                    if let Err(e) = apply_snapshot(update_msg) {
                        eprintln!("{}: {:?}", "apply snapshot failed".red().bold(), e);
                    }

                    let cur_res = execute_test(
                        moonbuild_opt,
                        moonc_opt.build_opt.target_backend,
                        artifact_path,
                        target_dir,
                        &test_args,
                        file_test_info_map,
                    )
                    .await?
                    .first()
                    .unwrap()
                    .clone();

                    // update the previous test result
                    *item = cur_res;
                }
            }
            Err(TestFailedStatus::ApplyExpectFailed(_)) => {
                eprintln!(
                    "{}: {:?}",
                    "failed to apply patch for expect testing".red().bold(),
                    "unexpected error"
                );
            }
            Err(TestFailedStatus::RuntimeError(err_ts) | TestFailedStatus::Failed(err_ts)) => {
                if output_failure_in_json {
                    println!("{}", serde_json_lenient::to_string(err_ts)?);
                } else {
                    println!(
                        "test {}/{}::{} {}: {}",
                        err_ts.package,
                        err_ts.filename,
                        err_ts.test_name,
                        "failed".bold().red(),
                        err_ts.message,
                    );
                }
            }
            Err(TestFailedStatus::Others(e)) => {
                eprintln!("{}: {}", "failed".red(), e);
            }
            Err(TestFailedStatus::ExpectTestFailed(origin_err)) => {
                if !auto_update {
                    if output_failure_in_json {
                        println!("{}", serde_json_lenient::to_string(&origin_err)?);
                    } else {
                        println!(
                            "test {}/{}::{} {}",
                            origin_err.package,
                            origin_err.filename,
                            origin_err.test_name,
                            "failed".bold().red(),
                        );
                    }
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
                    let index = origin_err.index.clone().parse::<u32>().unwrap();
                    let filename = origin_err.filename.clone();

                    let test_args = TestArgs {
                        package: origin_err.package.clone(),
                        file_and_index: vec![(filename, index..(index + 1))],
                    };
                    let rerun = execute_test(
                        moonbuild_opt,
                        moonc_opt.build_opt.target_backend,
                        artifact_path,
                        target_dir,
                        &test_args,
                        file_test_info_map,
                    )
                    .await?
                    .first()
                    .unwrap()
                    .clone();
                    let update_msg = match rerun {
                        // if rerun test success, update the previous test result and continue
                        Ok(_) => {
                            *item = rerun;
                            continue;
                        }
                        Err(TestFailedStatus::ExpectTestFailed(cur_err)) => &[cur_err.message],
                        _ => &[origin_err.message.clone()],
                    };

                    if let Err(e) = crate::expect::apply_expect(update_msg) {
                        eprintln!("{}: {:?}", "apply expect failed".red().bold(), e);
                    }

                    // recompile after apply expect
                    {
                        let state =
                            crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
                        let result = n2_run_interface(state, moonbuild_opt)?;
                        if result.is_none() {
                            break;
                        }
                    }

                    let mut cur_res = execute_test(
                        moonbuild_opt,
                        moonc_opt.build_opt.target_backend,
                        artifact_path,
                        target_dir,
                        &test_args,
                        file_test_info_map,
                    )
                    .await?
                    .first()
                    .unwrap()
                    .clone();

                    let mut cnt = 1;
                    let limit = moonbuild_opt.test_opt.as_ref().map(|it| it.limit).unwrap();
                    let mut rerun_error = false;
                    while let Err(TestFailedStatus::ExpectTestFailed(ref etf)) = cur_res {
                        if cnt >= limit {
                            break;
                        }

                        if let Err(e) = crate::expect::apply_expect(&[etf.message.clone()]) {
                            eprintln!("{}: {:?}", "failed".red().bold(), e);
                            break;
                        }

                        // recompile after apply expect
                        {
                            let state =
                                crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
                            let result = n2_run_interface(state, moonbuild_opt)?;
                            if result.is_none() {
                                rerun_error = true;
                                break;
                            }
                        }

                        cur_res = execute_test(
                            moonbuild_opt,
                            moonc_opt.build_opt.target_backend,
                            artifact_path,
                            target_dir,
                            &test_args,
                            file_test_info_map,
                        )
                        .await?
                        .first()
                        .unwrap()
                        .clone();

                        cnt += 1;
                    }

                    if rerun_error {
                        break;
                    }

                    // update the previous test result
                    *item = cur_res;
                }
            }
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
    write_pkg_lst(module, &moonbuild_opt.raw_target_dir)?;
    render_result(result, moonbuild_opt.quiet, "bundle")
}

pub fn run_fmt(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<i32> {
    let n2_input = super::fmt::gen_fmt(module, moonc_opt, moonbuild_opt)?;
    let state = if moonbuild_opt.fmt_opt.as_ref().unwrap().check {
        super::fmt::gen_n2_fmt_check_state(&n2_input, moonc_opt, moonbuild_opt)?
    } else {
        super::fmt::gen_n2_fmt_state(&n2_input, moonc_opt, moonbuild_opt)?
    };
    let res = n2_run_interface(state, moonbuild_opt)?;

    match res {
        None => {
            return Ok(1);
        }
        Some(0) => (),
        Some(_) => (),
    }
    Ok(0)
}
