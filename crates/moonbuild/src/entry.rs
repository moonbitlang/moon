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

use crate::benchmark::{BATCHBENCH, render_batch_bench_summary};
use crate::check::normal::write_pkg_lst;
use crate::expect::{apply_snapshot, render_snapshot_fail};
use crate::runtest::TestStatistics;
use crate::test_utils::indices_to_ranges;

use moonutil::common::{
    DOT_MBT_DOT_MD, DiagnosticLevel, DriverKind, FileLock, FileName, MbtTestInfo, MoonbuildOpt,
    MooncGenTestInfo, MooncOpt, PrePostBuild, TEST_INFO_FILE, TargetBackend, TestArtifacts,
    TestBlockIndex,
};

use std::sync::{Arc, Mutex};

fn default_parallelism() -> anyhow::Result<usize> {
    let par = std::thread::available_parallelism()?;
    Ok(usize::from(par))
}

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
    fn append_content(&mut self, s: impl Into<String>, report: Option<ReportKind>) {
        self.content_writer.push(s.into());
        match report {
            Some(ReportKind::Error) => self.n_errors += 1,
            Some(ReportKind::Warning) => self.n_warnings += 1,
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)] // This is inefficient and we know it
pub fn render_and_catch_callback(
    catcher: Arc<Mutex<ResultCatcher>>,
    no_render_output: bool,
    use_fancy: bool,
    check_patch_file: Option<PathBuf>,
    explain: bool,
    render_no_loc: DiagnosticLevel,
    source_dir: PathBuf,
    target_dir: PathBuf,
) -> impl Fn(&str) {
    move |output: &str| {
        output
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                let report_kind = if no_render_output {
                    println!("{content}");
                    None
                } else {
                    moonutil::render::MooncDiagnostic::render(
                        content,
                        use_fancy,
                        check_patch_file.clone(),
                        explain,
                        render_no_loc,
                        &source_dir,
                        &target_dir,
                    )
                };
                catcher.lock().unwrap().append_content(content, report_kind);
            });
    }
}

pub fn n2_simple_run_interface(
    state: n2::load::State,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<Option<usize>> {
    let logger = Arc::new(Mutex::new(ResultCatcher::default()));
    let use_fancy = terminal::use_fancy();

    let output_json = moonbuild_opt.no_render_output;
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
    let render_no_loc = moonbuild_opt.render_no_loc;
    let render_and_catch = render_and_catch_callback(
        Arc::clone(&logger),
        output_json,
        use_fancy,
        check_patch_file,
        explain,
        render_no_loc,
        source_dir,
        target_dir,
    );

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
    } else {
        match default_parallelism() {
            Ok(val) => Ok(val),
            _ => {
                warn!(
                    "Failed to get the parallelism for building, falling back to 1 parallel task"
                );
                Ok(1)
            }
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

pub fn n2_run_interface(
    state: n2::load::State,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2RunStats> {
    let logger = Arc::new(Mutex::new(ResultCatcher::default()));
    let use_fancy = terminal::use_fancy();

    let no_render_output = moonbuild_opt.no_render_output;
    let check_patch_file = moonbuild_opt
        .check_opt
        .as_ref()
        .and_then(|it| it.patch_file.clone());
    let explain = moonbuild_opt
        .check_opt
        .as_ref()
        .is_some_and(|it| it.explain);

    let target_dir = moonbuild_opt.target_dir.clone();
    let source_dir = moonbuild_opt.source_dir.clone();
    let render_no_loc = moonbuild_opt.render_no_loc;
    let render_and_catch = render_and_catch_callback(
        Arc::clone(&logger),
        no_render_output,
        use_fancy,
        check_patch_file,
        explain,
        render_no_loc,
        source_dir.clone(),
        target_dir.clone(),
    );

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

    if let Some(0) = res {
        // if no work to do, then do not rewrite (build | check | test ...).output
        // instead, read it and print
        let raw_json = std::fs::read_to_string(&output_path)
            .context(format!("failed to open `{}`", output_path.display()))?;

        let check_patch_file = moonbuild_opt
            .check_opt
            .as_ref()
            .and_then(|it| it.patch_file.clone());

        let callback = render_and_catch_callback(
            Arc::clone(&logger),
            no_render_output,
            use_fancy,
            check_patch_file,
            explain,
            render_no_loc,
            source_dir,
            target_dir,
        );

        raw_json.lines().for_each(callback);
    } else {
        let mut output_file = std::fs::File::create(output_path)?;

        for item in logger.lock().unwrap().content_writer.iter() {
            output_file.write_all(item.as_bytes())?;
            output_file.write_all("\n".as_bytes())?;
        }
    }

    let logger = logger.lock().unwrap();
    Ok(N2RunStats {
        n_tasks_executed: res,
        n_errors: logger.n_errors,
        n_warnings: logger.n_warnings,
    })
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
    render_result(&result, moonbuild_opt.quiet, "checking")
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
    render_result(&result, moonbuild_opt.quiet, "building")
}

/// Run a package without holding a lock during subprocess execution.
/// For backward compatibility, this function calls `run_run_with_lock` with `None` for the lock parameter.
/// The lock should be managed by the caller if lock management during the subprocess execution is needed.
pub fn run_run(
    package_path: &str,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
    build_only: bool,
) -> anyhow::Result<i32> {
    run_run_with_lock(
        package_path,
        moonc_opt,
        moonbuild_opt,
        module,
        build_only,
        None,
    )
}

/// Run a package with an optional lock that will be dropped after the build phase.
///
/// The lock is released immediately after the build completes, allowing other moon commands
/// to run concurrently during the subprocess execution. This is particularly important for
/// long-running programs started via `moon run`.
pub fn run_run_with_lock(
    package_path: &str,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
    build_only: bool,
    lock: Option<FileLock>,
) -> anyhow::Result<i32> {
    run_build(moonc_opt, moonbuild_opt, module)?;

    // Release the lock after build completes and before any subprocess spawning.
    // This allows other moon commands to acquire the lock and run concurrently.
    // All subsequent operations (path resolution, artifact determination, etc.)
    // do not require the lock as they operate on already-built artifacts.
    drop(lock);

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

pub type FileTestInfo = IndexMap<FileName, IndexMap<TestBlockIndex, MbtTestInfo>>;

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
            .chain(info.async_tests.into_iter())
        {
            if test_info.is_empty() {
                continue;
            }
            if let Some(filter_file) = filter_file
                && filename != *filter_file
            {
                continue;
            }
            current_pkg_test_info
                .entry((artifact_path.clone(), driver_kind))
                .or_insert(IndexMap::new())
                .entry(filename)
                .or_insert(IndexMap::new())
                .extend(test_info.into_iter().map(|it| (it.index, it)));
        }
    }

    if sort_input {
        current_pkg_test_info.sort_keys();
    }

    Ok(current_pkg_test_info)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::single_range_in_vec_init)]
pub fn run_test(
    moonc_opt: MooncOpt,
    moonbuild_opt: MoonbuildOpt,
    build_only: bool,
    test_verbose_output: bool,
    auto_update: bool,
    module: ModuleDB,
    include_skipped: bool,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let moonc_opt = Arc::new(moonc_opt);
    let moonbuild_opt = Arc::new(moonbuild_opt);
    let module = Arc::new(module);

    let state = crate::runtest::load_moon_proj(&module, &moonc_opt, &moonbuild_opt)?;
    let result = n2_run_interface(state, &moonbuild_opt)?;
    render_result(&result, moonbuild_opt.quiet, "testing")?;

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
        if let Some(package) = filter_package
            && !package.contains(pkgname)
        {
            continue;
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
            for (file_name, test_metadata) in &file_test_info_map {
                let filter_index = filter_index.or(filter_doc_index);
                if let Some(filter_index) = filter_index {
                    // Single test filter - use exact index
                    // for single test, the `#skip` attribute is ignored
                    let ranges = vec![filter_index..(filter_index + 1)];
                    test_args.file_and_index.push((file_name.clone(), ranges));
                } else {
                    // No filter - use actual indices from metadata, filtering based on include_skipped
                    let actual_indices: Vec<u32> = test_metadata
                        .values()
                        .filter(|t| include_skipped || !t.has_skip())
                        .map(|t| t.index)
                        .collect();
                    let ranges = indices_to_ranges(actual_indices);
                    test_args.file_and_index.push((file_name.clone(), ranges));
                }
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
    pub file_and_index: Vec<(String, Vec<std::ops::Range<u32>>)>,
}

impl TestArgs {
    fn get_test_cnt(&self) -> u32 {
        self.file_and_index
            .iter()
            .map(|(_, ranges)| {
                ranges
                    .iter()
                    .map(|range| range.end - range.start)
                    .sum::<u32>()
            })
            .sum()
    }

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

// FIXME: This should be completely rewritten. In particular, the Result usage is wrong.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::single_range_in_vec_init)] // clippy warns about our ranges
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
        // Helper to get test info for any test statistics
        let get_test_info = |stats: &TestStatistics| -> Option<&MbtTestInfo> {
            let parsed_index = stats.index.parse::<u32>().ok()?;
            file_test_info_map
                .get(&stats.filename)
                .and_then(|submap| submap.get(&parsed_index))
        };

        match item {
            Ok(ok_ts) => {
                let info = get_test_info(ok_ts);
                let formatter = CompactTestFormatter::new(&module.name, ok_ts, info);

                if ok_ts.message.starts_with(BATCHBENCH) {
                    let _ = formatter.write_bench(&mut std::io::stdout());
                    println!();
                    render_batch_bench_summary(&ok_ts.message);
                } else if test_verbose_output {
                    let _ = formatter.write_success(&mut std::io::stdout());
                    println!();
                }
            }
            Err(TestFailedStatus::SnapshotPending(stat)) => {
                if !auto_update {
                    if output_failure_in_json {
                        println!("{}", serde_json_lenient::to_string(stat)?);
                    } else {
                        let info = get_test_info(stat);
                        let formatter = CompactTestFormatter::new(&module.name, stat, info);
                        let _ = formatter.write_failure(&mut std::io::stdout());
                        println!();
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
                    apply_snapshot([stat.message.as_str()])?;
                    let index = stat.index.clone().parse::<u32>().unwrap();
                    let test_args = TestArgs {
                        package: stat.package.clone(),
                        file_and_index: vec![(stat.filename.clone(), vec![index..(index + 1)])],
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

                    let update_msg = match &rerun {
                        // if rerun test success, update the previous test result and continue
                        Ok(_) => {
                            *item = rerun;
                            continue;
                        }
                        Err(TestFailedStatus::SnapshotPending(cur_err)) => &cur_err.message,
                        _ => &stat.message,
                    };
                    if let Err(e) = apply_snapshot([update_msg.as_str()]) {
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
                    let info = get_test_info(err_ts);
                    let formatter = CompactTestFormatter::new(&module.name, err_ts, info);
                    let _ = formatter
                        .write_failure_with_message(&mut std::io::stdout(), &err_ts.message);
                    println!();
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
                        let info = get_test_info(origin_err);
                        let formatter = CompactTestFormatter::new(&module.name, origin_err, info);
                        let _ = formatter.write_failure(&mut std::io::stdout());
                        println!();
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
                        file_and_index: vec![(filename, vec![index..(index + 1)])],
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
                    let update_msg = match &rerun {
                        // if rerun test success, update the previous test result and continue
                        Ok(_) => {
                            *item = rerun;
                            continue;
                        }
                        Err(TestFailedStatus::ExpectTestFailed(cur_err)) => &cur_err.message,
                        _ => &origin_err.message,
                    };

                    if let Err(e) = crate::expect::apply_expect([update_msg.as_str()]) {
                        eprintln!("{}: {:?}", "apply expect failed".red().bold(), e);
                    }

                    // recompile after apply expect
                    {
                        let state =
                            crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
                        let result = n2_run_interface(state, moonbuild_opt)?;
                        if result.n_tasks_executed.is_none() {
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

                        if let Err(e) = crate::expect::apply_expect([etf.message.as_str()]) {
                            eprintln!("{}: {:?}", "failed".red().bold(), e);
                            break;
                        }

                        // recompile after apply expect
                        {
                            let state =
                                crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?;
                            let result = n2_run_interface(state, moonbuild_opt)?;
                            if result.n_tasks_executed.is_none() {
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
    render_result(&result, moonbuild_opt.quiet, "bundle")?;
    Ok(0)
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

    match res.n_tasks_executed {
        None => Ok(1),
        Some(_) => Ok(0),
    }
}
