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

//! Lowering context and core implementation.

use std::path::PathBuf;

use log::debug;
use moonutil::{
    constants::{MOON_MOD, MOON_MOD_JSON},
    resolution::{DirSyncResult, ModuleId, ModuleSourceKind, ResolvedEnv},
};
use n2::graph::{Build, Graph as N2Graph};
use tracing::{Level, instrument};

use crate::{
    ResolveOutput,
    build_action_plan::{BuildAction, BuildActionId, BuildActionPlan, BuildProduct},
    dependency_build_cache::{
        DependencyBuildAction, DependencyBuildDescription, DependencyBuildInput,
        DependencyBuildOutput, DependencyResolution, InputIdentity,
    },
    discover::{DiscoverResult, DiscoveredPackage},
    model::BuildTarget,
    pkg_solve::DepRelationship,
    target_layout::ArtifactPathResolver,
};
use moonutil::toolchain::BINARIES;

use super::{
    BuildCommand, BuildOptions, CommandArgMap, LoweringError,
    utils::{build_ins, build_n2_fileloc, build_outs},
};

pub(crate) struct LoweringContext<'a> {
    // What we're building
    pub(crate) graph: N2Graph,

    pub(crate) command_args_by_output: CommandArgMap,

    pub(crate) dependency_build_actions: Vec<DependencyBuildAction>,
    /// False when an action in the reusable registry dependency graph could
    /// not be represented without invocation-local state.
    pub(crate) dependency_build_actions_complete: bool,

    // Physical paths for logical build products.
    pub(crate) artifact_paths: ArtifactPathResolver,

    // External state
    pub(crate) packages: &'a DiscoverResult,
    pub(crate) modules: &'a ResolvedEnv,
    pub(crate) module_dirs: &'a DirSyncResult,
    pub(crate) prepared_sources: &'a mooncake::prepared_source::PreparedSourceMap,
    pub(crate) rel: &'a DepRelationship,
    pub(crate) plan: &'a BuildActionPlan<'a>,
    pub(crate) opt: &'a BuildOptions,
}

pub(super) struct ActionProducts {
    outputs: Vec<RealizedProduct>,
    dependencies: Vec<RealizedProduct>,
}

enum DependencyBuildCandidate {
    NotDependency,
    Cacheable(DependencyBuildDescription),
    Uncacheable { inputs: Vec<DependencyBuildInput> },
}

struct RealizedProduct {
    product: BuildProduct,
    paths: Vec<PathBuf>,
}

impl ActionProducts {
    fn new(ctx: &LoweringContext<'_>, action: BuildActionId) -> Self {
        let outputs = ctx
            .plan
            .output_products(action)
            .into_iter()
            .map(|product| Self::realize(ctx, action, product))
            .collect();
        let dependencies = ctx
            .plan
            .dependency_products(action)
            .into_iter()
            .map(|(dependency_action, product)| Self::realize(ctx, dependency_action, product))
            .collect();
        Self {
            outputs,
            dependencies,
        }
    }

    fn realize(
        ctx: &LoweringContext<'_>,
        product_action: BuildActionId,
        product: BuildProduct,
    ) -> RealizedProduct {
        let paths = ctx.artifact_paths.paths_for_product(
            &product,
            ctx.plan.action(product_action),
            ctx.packages,
            ctx.modules,
            ctx.opt.artifact_path_options(),
        );
        RealizedProduct { product, paths }
    }

    pub(super) fn dependency_paths(&self) -> Vec<PathBuf> {
        Self::paths(&self.dependencies)
    }

    pub(super) fn output_paths(&self) -> Vec<PathBuf> {
        Self::paths(&self.outputs)
    }

    pub(super) fn single_output_path(&self) -> PathBuf {
        match self.outputs.as_slice() {
            [product] => Self::optional_single_realized_path(product)
                .unwrap_or_else(|| unreachable!("expected exactly one path for product")),
            [] => unreachable!("expected exactly one output product"),
            _ => unreachable!(
                "expected one output product, got {:?}",
                self.outputs
                    .iter()
                    .map(|realized| &realized.product)
                    .collect::<Vec<_>>()
            ),
        }
    }

    pub(super) fn single_output_path_matching(
        &self,
        matches: impl Fn(&BuildProduct) -> bool,
    ) -> PathBuf {
        self.optional_single_output_path_matching(matches)
            .unwrap_or_else(|| unreachable!("expected one matching output product"))
    }

    pub(super) fn optional_single_output_path_matching(
        &self,
        matches: impl Fn(&BuildProduct) -> bool,
    ) -> Option<PathBuf> {
        Self::single_matching_path(&self.outputs, matches)
    }

    pub(super) fn single_dependency_path_matching(
        &self,
        matches: impl Fn(&BuildProduct) -> bool,
    ) -> PathBuf {
        Self::single_matching_path(&self.dependencies, matches)
            .unwrap_or_else(|| unreachable!("expected one matching dependency product"))
    }

    pub(super) fn dependency_paths_matching(
        &self,
        matches: impl Fn(&BuildProduct) -> bool,
    ) -> Vec<PathBuf> {
        self.dependencies
            .iter()
            .filter(|realized| matches(&realized.product))
            .flat_map(|realized| realized.paths.iter().cloned())
            .collect()
    }

    fn paths(realized: &[RealizedProduct]) -> Vec<PathBuf> {
        realized
            .iter()
            .flat_map(|product| product.paths.iter().cloned())
            .collect()
    }

    fn single_matching_path(
        realized: &[RealizedProduct],
        matches: impl Fn(&BuildProduct) -> bool,
    ) -> Option<PathBuf> {
        let matched = realized
            .iter()
            .filter(|realized| matches(&realized.product))
            .collect::<Vec<_>>();
        match matched.as_slice() {
            [product] => Self::optional_single_realized_path(product),
            [] => None,
            _ => unreachable!("expected at most one matching product"),
        }
    }

    fn optional_single_realized_path(product: &RealizedProduct) -> Option<PathBuf> {
        match product.paths.as_slice() {
            [path] => Some(path.clone()),
            [] => None,
            _ => unreachable!(
                "expected one path for product, got {:?}: {:?}",
                product.paths, product.product
            ),
        }
    }
}

/// Canonicalize the structured `moonc build-package` argv without rewriting
/// arbitrary substrings. Most path arguments are standalone values; `-i` and
/// `-pkg-sources` are the two documented compound forms.
fn canonicalize_dependency_command_args(
    args: &[String],
    path_bindings: &[(String, String)],
) -> Option<Vec<String>> {
    let path_value = |value: &str| {
        path_bindings.iter().find_map(|(path, label)| {
            value.strip_prefix(path).and_then(|suffix| {
                (suffix.is_empty() || suffix.starts_with('/') || suffix.starts_with('\\'))
                    .then(|| format!("{label}{suffix}"))
            })
        })
    };
    let package_source = |value: &str| {
        path_bindings.iter().find_map(|(path, label)| {
            value
                .strip_suffix(path)
                .and_then(|prefix| prefix.ends_with(':').then(|| format!("{prefix}{label}")))
        })
    };
    let mi_dependency = |value: &str| {
        path_bindings.iter().find_map(|(path, label)| {
            value
                .strip_prefix(path)
                .and_then(|suffix| suffix.strip_prefix(':'))
                .map(|alias| {
                    let alias = path_value(alias).unwrap_or_else(|| alias.to_owned());
                    format!("{label}:{alias}")
                })
        })
    };
    let prefixed_path = |value: &str| {
        [
            "-Wl,-rpath,",
            "-Wl,-rpath=",
            "/LIBPATH:",
            "/libpath:",
            "/Out:",
            "/OUT:",
            "/Fo",
            "/Fe",
            "/Fd",
            "/I",
            "-isystem",
            "-I",
            "-L",
        ]
        .into_iter()
        .find_map(|prefix| {
            value
                .strip_prefix(prefix)
                .and_then(&path_value)
                .map(|path| format!("{prefix}{path}"))
        })
    };

    let mut canonical = Vec::with_capacity(args.len());
    for (index, argument) in args.iter().enumerate() {
        let previous = index.checked_sub(1).and_then(|index| args.get(index));
        if let Some(label) = path_value(argument) {
            canonical.push(label);
        } else if previous.is_some_and(|previous| previous == "-i")
            && let Some(value) = mi_dependency(argument)
        {
            canonical.push(value);
        } else if previous.is_some_and(|previous| previous == "-pkg-sources")
            && let Some(value) = package_source(argument)
        {
            canonical.push(value);
        } else if let Some(value) = prefixed_path(argument) {
            canonical.push(value);
        } else if path_bindings
            .iter()
            .any(|(path, _)| argument.contains(path))
        {
            // Unknown compound path syntax is not safe to share across
            // invocation-local target/source directories.
            return None;
        } else {
            canonical.push(argument.clone());
        }
    }
    Some(canonical)
}

impl<'a> LoweringContext<'a> {
    pub(super) fn new(
        artifact_paths: ArtifactPathResolver,
        resolve_output: &'a ResolveOutput,
        plan: &'a BuildActionPlan<'a>,
        opt: &'a BuildOptions,
    ) -> Self {
        Self {
            graph: N2Graph::default(),
            command_args_by_output: CommandArgMap::new(),
            dependency_build_actions: Vec::new(),
            dependency_build_actions_complete: true,
            artifact_paths,
            rel: &resolve_output.pkg_rel,
            modules: &resolve_output.module_rel,
            packages: &resolve_output.pkg_dirs,
            module_dirs: &resolve_output.module_dirs,
            prepared_sources: &resolve_output.prepared_sources,
            plan,
            opt,
        }
    }

    /// Some actions are no-op in n2 build graph. Early bailing.
    fn is_action_noop(&self, action: BuildAction<'_>) -> bool {
        (!self.opt.target_backend.is_native())
            && matches!(action, BuildAction::MakeExecutable { .. })
    }

    pub(super) fn get_package(&self, target: BuildTarget) -> &DiscoveredPackage {
        self.packages.get_package(target.package)
    }

    pub(super) fn output_paths_for_action(&self, action: BuildActionId) -> Vec<PathBuf> {
        self.plan
            .output_products(action)
            .into_iter()
            .flat_map(|product| {
                self.artifact_paths.paths_for_product(
                    &product,
                    self.plan.action(action),
                    self.packages,
                    self.modules,
                    self.opt.artifact_path_options(),
                )
            })
            .collect()
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_action(&mut self, id: BuildActionId) -> Result<(), LoweringError> {
        let action = self.plan.action(id);
        if self.is_action_noop(action) {
            return Ok(());
        }
        let action_products = ActionProducts::new(self, id);

        // Lower the action to its command and tool-specific execution transport.
        let mut cmd = match action {
            BuildAction::Check { target, info } => {
                self.lower_check(&action_products, target, info)?
            }
            BuildAction::EmitProof { target, info } => {
                self.lower_emit_proof(&action_products, target, info)?
            }
            BuildAction::Prove { target, info } => {
                self.lower_prove(&action_products, target, info)?
            }
            BuildAction::BuildCore { target, info } => {
                self.lower_build_mbt(&action_products, target, info)?
            }
            BuildAction::BuildCStub {
                package,
                index,
                info,
            } => self.lower_build_c_stub(&action_products, package, index, info),
            BuildAction::ArchiveOrLinkCStubs { package, info } => {
                self.lower_archive_or_link_c_stubs(&action_products, package, info)
            }
            BuildAction::LinkCore {
                target,
                info,
                make_executable_info,
            } => self.lower_link_core(&action_products, target, info, make_executable_info)?,
            BuildAction::MakeExecutable {
                target,
                info: Some(info),
            } => self.lower_make_exe(&action_products, target, info),
            BuildAction::MakeExecutable { info: None, .. } => {
                panic!("native MakeExecutable actions should have executable info")
            }
            BuildAction::GenerateTestInfo { target, info } => {
                self.lower_gen_test_driver(&action_products, target, info)
            }
            BuildAction::GenerateMbti { target } => {
                self.lower_generate_mbti(&action_products, target)
            }
            BuildAction::BuildVirtual { package } => self.lower_parse_mbti(package)?,
            BuildAction::Bundle { module, targets } => {
                self.lower_bundle(&action_products, module, targets)?
            }
            BuildAction::BuildRuntimeLib { info } => {
                self.lower_compile_runtime(&action_products, info)
            }
            BuildAction::BuildDocs { module } => self.lower_build_docs(module),
            BuildAction::RunPrebuild { info, .. } => self.lower_run_prebuild(info),
            BuildAction::RunMoonLexPrebuild { package, index } => {
                self.lower_moon_lex_prebuild(package, index)
            }
            BuildAction::RunMoonYaccPrebuild { package, index } => {
                self.lower_moon_yacc_prebuild(package, index)
            }
        };

        if self.opt.collect_dependency_build_actions
            && let Some(cwd) = self.dependency_build_working_directory(action)
        {
            // C debug information can record the compiler's working directory.
            // Run reusable package actions from their shared prepared source
            // instead of leaking the standalone invocation's directory.
            debug_assert!(
                cmd.commandline.cwd.is_none(),
                "registry dependency action already has a working directory"
            );
            cmd.commandline.cwd = Some(cwd);
        }

        // Collect n2 inputs and outputs.
        //
        // MAINTAINERS: some of the inputs and outputs might be calculated
        // twice, once for the commandline and another here. This is currently
        // not a performance concern, but if you have found a way to optimize
        // this, or if you are duplicating a lot of code for it, please refactor.
        let dependency_build = if self.opt.collect_dependency_build_actions {
            self.dependency_build_description(action, &action_products, &cmd)
        } else {
            DependencyBuildCandidate::NotDependency
        };
        let mut ins = action_products.dependency_paths();
        ins.extend(cmd.extra_inputs.iter().cloned());
        // Track tool binary dependencies so that n2 detects when compilers
        // or other toolchain binaries change (e.g. after a toolchain update)
        // and triggers a rebuild.
        if self.plan.needs_moonc_tool_dep(id) {
            ins.push(BINARIES.moonc.clone());
        }
        let dependency_inputs: &[DependencyBuildInput] = match &dependency_build {
            DependencyBuildCandidate::Cacheable(description) => &description.inputs,
            DependencyBuildCandidate::Uncacheable { inputs } => inputs,
            DependencyBuildCandidate::NotDependency => &[],
        };
        for input in dependency_inputs {
            if !ins.contains(&input.path) {
                ins.push(input.path.clone());
            }
        }
        ins.sort(); // make sure the order is deterministic
        let ins = build_ins(&mut self.graph, ins);

        let output_paths = action_products.output_paths();
        if let Some(args) = cmd.commandline.args() {
            for output_path in &output_paths {
                self.command_args_by_output
                    .insert(output_path.clone(), args.clone());
            }
        }
        let mut commandline = cmd.commandline;
        let cwd = commandline.cwd.take();
        let env = std::mem::take(&mut commandline.env);
        let (n2_command, rspfile) = commandline.into_n2();
        let outs = build_outs(&mut self.graph, output_paths);

        // Construct n2 build node
        let mut build = Build::new(
            build_n2_fileloc(self.plan.fileloc(id, self.modules, self.packages)),
            ins,
            outs,
        );
        build.cmdline = Some(n2_command);
        build.rspfile = rspfile;
        build.cwd = cwd.map(|cwd| cwd.display().to_string());
        build.env = env;
        build.desc = Some(self.plan.human_desc(id, self.modules, self.packages));
        // n2 can't capture and replay command outputs. this is a workaround to
        // avoid losing warnings from `moonc`. According to legacy code, this
        // only triggers for `Check` nodes.
        //
        // FIXME: Revisit for other `moonc` invocations, e.g. `BuildCore`.
        build.can_dirty_on_output = self.plan.can_dirty_on_output(id);

        self.debug_print_command_and_files(id, &build);
        let fqn = self
            .plan
            .package_for_error(id)
            .map(|x| self.get_package(x).fqn.clone());
        let build_id = self.graph.add_build(build).map_err(|e| LoweringError::N2 {
            package: fqn.into(),
            action: id,
            source: e,
        })?;
        match dependency_build {
            DependencyBuildCandidate::Cacheable(description) => {
                self.dependency_build_actions.push(DependencyBuildAction {
                    build_id,
                    description,
                });
            }
            DependencyBuildCandidate::Uncacheable { .. } => {
                self.dependency_build_actions_complete = false;
            }
            DependencyBuildCandidate::NotDependency => {}
        }
        Ok(())
    }

    fn dependency_build_working_directory(&self, action: BuildAction<'_>) -> Option<PathBuf> {
        let package = match action {
            BuildAction::BuildCore { target, .. }
                if target.kind == crate::model::TargetKind::Source =>
            {
                Some(self.get_package(target))
            }
            BuildAction::BuildCStub { package, .. }
            | BuildAction::ArchiveOrLinkCStubs { package, .. } => {
                Some(self.packages.get_package(package))
            }
            BuildAction::BuildRuntimeLib { .. } if !self.prepared_sources.is_empty() => {
                return Some(
                    self.opt
                        .runtime_dot_c_path()
                        .parent()
                        .expect("runtime source should have a parent directory")
                        .to_path_buf(),
                );
            }
            _ => None,
        }?;
        matches!(
            self.modules.module_source(package.module).source(),
            ModuleSourceKind::Registry
        )
        .then(|| package.root_path.clone())
    }

    fn dependency_build_description(
        &self,
        action: BuildAction<'_>,
        products: &ActionProducts,
        cmd: &BuildCommand,
    ) -> DependencyBuildCandidate {
        use crate::dependency_build_cache::DependencyBuildKind;

        let (kind, package, build_core_target) = match action {
            BuildAction::BuildCore { target, .. }
                if target.kind == crate::model::TargetKind::Source =>
            {
                (
                    DependencyBuildKind::MooncBuildCore,
                    Some(self.get_package(target)),
                    Some(target),
                )
            }
            BuildAction::BuildCStub { package, .. } => (
                DependencyBuildKind::CStubObject,
                Some(self.packages.get_package(package)),
                None,
            ),
            BuildAction::ArchiveOrLinkCStubs { package, .. } => (
                DependencyBuildKind::CStubLibrary,
                Some(self.packages.get_package(package)),
                None,
            ),
            BuildAction::BuildRuntimeLib { .. } if !self.prepared_sources.is_empty() => {
                (DependencyBuildKind::NativeRuntime, None, None)
            }
            _ => return DependencyBuildCandidate::NotDependency,
        };
        if package.is_some_and(|package| {
            !matches!(
                self.modules.module_source(package.module).source(),
                ModuleSourceKind::Registry
            )
        }) {
            return DependencyBuildCandidate::NotDependency;
        }
        let Some(working_directory) = cmd.commandline.cwd.as_ref() else {
            return DependencyBuildCandidate::Uncacheable { inputs: Vec::new() };
        };
        let Some(args) = cmd.commandline.args().cloned() else {
            return DependencyBuildCandidate::Uncacheable { inputs: Vec::new() };
        };

        let module = package.map(|package| self.modules.module_source(package.module));
        let module_dir = package.map(|package| &self.module_dirs[package.module]);
        let target_dir = self.artifact_paths.target_layout().target_base_dir();
        let all_pkgs = self
            .artifact_paths
            .target_layout()
            .all_pkgs_of_build_target(self.opt.target_backend.into());

        let mut inputs = products
            .dependencies
            .iter()
            .filter_map(|realized| {
                self.product_label(&realized.product)
                    .map(|label| (label, &realized.paths))
            })
            .flat_map(|(label, paths)| {
                paths.iter().cloned().map(move |path| DependencyBuildInput {
                    label: label.clone(),
                    path,
                    identity: InputIdentity::Logical,
                })
            })
            .collect::<Vec<_>>();

        for (index, path) in cmd.extra_inputs.iter().enumerate() {
            let package_relative = package.and_then(|package| {
                path.strip_prefix(&package.root_path)
                    .ok()
                    .map(|relative| (package, relative))
            });
            let module_relative = module_dir.and_then(|module_dir| {
                path.strip_prefix(module_dir)
                    .ok()
                    .map(|relative| (module.expect("module directory requires module"), relative))
            });
            let label = match (package_relative, module_relative) {
                (Some((package, relative)), _) => format!(
                    "module:{}@{}:package:{}:{}",
                    module.expect("registry package requires module").name(),
                    module.expect("registry package requires module").version(),
                    package.fqn,
                    relative.to_string_lossy()
                ),
                (None, Some((module, relative))) => format!(
                    "module:{}@{}:{}",
                    module.name(),
                    module.version(),
                    relative.to_string_lossy()
                ),
                (None, None) => format!(
                    "external-input:{index}:{}",
                    path.file_name()
                        .map(|name| name.to_string_lossy())
                        .unwrap_or_default()
                ),
            };
            inputs.push(DependencyBuildInput {
                label,
                path: path.clone(),
                identity: if module_dir.is_some_and(|module_dir| path.starts_with(module_dir))
                    || path.starts_with(target_dir)
                    || path == &all_pkgs
                {
                    InputIdentity::Logical
                } else {
                    InputIdentity::Content
                },
            });
        }
        if let Some(target) = build_core_target {
            for (index, dependency) in self.mi_inputs_of(target).into_iter().enumerate() {
                let path = dependency.path.into_owned();
                if !inputs.iter().any(|input| input.path == path) {
                    inputs.push(DependencyBuildInput {
                        label: format!(
                            "compiler-interface:{}",
                            dependency
                                .alias
                                .as_deref()
                                .map(str::to_owned)
                                .unwrap_or_else(|| index.to_string())
                        ),
                        identity: if path.starts_with(target_dir) {
                            InputIdentity::Logical
                        } else {
                            InputIdentity::Content
                        },
                        path,
                    });
                }
            }

            let module = module.expect("BuildCore dependency requires module");
            let module_dir = module_dir.expect("BuildCore dependency requires module directory");
            let module_manifest = if module_dir.join(MOON_MOD).is_file() {
                module_dir.join(MOON_MOD)
            } else {
                module_dir.join(MOON_MOD_JSON)
            };
            inputs.push(DependencyBuildInput {
                label: format!("module:{}@{}:manifest", module.name(), module.version()),
                path: module_manifest,
                identity: InputIdentity::Logical,
            });
        }
        let tool = match kind {
            DependencyBuildKind::MooncBuildCore => BINARIES.moonc.clone(),
            DependencyBuildKind::CStubObject
            | DependencyBuildKind::CStubLibrary
            | DependencyBuildKind::NativeRuntime => {
                let Some(tool) = args.first() else {
                    return DependencyBuildCandidate::Uncacheable { inputs };
                };
                PathBuf::from(tool)
            }
        };
        inputs.push(DependencyBuildInput {
            label: format!("tool:{kind:?}"),
            path: tool,
            identity: InputIdentity::Tool,
        });
        inputs.sort_by(|left, right| left.label.cmp(&right.label));
        inputs.dedup_by(|left, right| {
            left.label == right.label && left.path == right.path && left.identity == right.identity
        });

        let mut outputs = products
            .outputs
            .iter()
            .filter_map(|realized| {
                self.product_label(&realized.product)
                    .map(|label| (label, &realized.paths))
            })
            .flat_map(|(label, paths)| {
                paths
                    .iter()
                    .cloned()
                    .map(move |path| DependencyBuildOutput {
                        label: label.clone(),
                        path,
                    })
            })
            .collect::<Vec<_>>();
        outputs.sort_by(|left, right| left.label.cmp(&right.label));

        let mut path_bindings = inputs
            .iter()
            .map(|input| {
                (
                    input.path.to_string_lossy().into_owned(),
                    format!("$input:{}", input.label),
                )
            })
            .chain(outputs.iter().map(|output| {
                (
                    output.path.to_string_lossy().into_owned(),
                    format!("$output:{}", output.label),
                )
            }))
            .chain(package.into_iter().map(|package| {
                (
                    package.root_path.to_string_lossy().into_owned(),
                    "$package-root".to_owned(),
                )
            }))
            .chain(module_dir.into_iter().map(|module_dir| {
                (
                    module_dir.to_string_lossy().into_owned(),
                    "$module-root".to_owned(),
                )
            }))
            .chain([
                (
                    target_dir.to_string_lossy().into_owned(),
                    "$target-root".to_owned(),
                ),
                (
                    all_pkgs.to_string_lossy().into_owned(),
                    "$resolved-packages".to_owned(),
                ),
            ])
            .collect::<Vec<_>>();
        path_bindings.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
        let Some(canonical_args) = canonicalize_dependency_command_args(&args, &path_bindings)
        else {
            return DependencyBuildCandidate::Uncacheable { inputs };
        };
        let mut environment = cmd.commandline.env.clone();
        environment.sort();

        DependencyBuildCandidate::Cacheable(DependencyBuildDescription {
            kind,
            package: package.map_or_else(
                || "$native-runtime".to_owned(),
                |package| package.fqn.to_string(),
            ),
            working_directory: working_directory.display().to_string(),
            canonical_args,
            environment,
            resolution: package
                .map(|package| self.resolution_closure(package.module))
                .unwrap_or_default(),
            inputs,
            outputs,
        })
    }

    fn resolution_closure(&self, root: ModuleId) -> Vec<DependencyResolution> {
        let mut visited = std::collections::HashSet::new();
        let mut pending = vec![root];
        let mut resolution = Vec::new();

        while let Some(module) = pending.pop() {
            if !visited.insert(module) {
                continue;
            }
            let source = self.modules.module_source(module);
            resolution.push(DependencyResolution {
                module: source.name().to_string(),
                version: source.version().to_string(),
                source_checksum: self
                    .prepared_sources
                    .get(module)
                    .map(|prepared| prepared.verified_checksum().to_owned()),
            });
            pending.extend(self.modules.deps(module));
        }
        resolution.sort();
        resolution
    }

    fn product_label(&self, product: &BuildProduct) -> Option<String> {
        match product {
            BuildProduct::PackageInterface { target } => Some(format!(
                "package-interface:{}",
                self.get_package(*target).fqn
            )),
            BuildProduct::PackageCoreIr { target } => {
                Some(format!("package-core-ir:{}", self.get_package(*target).fqn))
            }
            BuildProduct::CStubObject { package, index } => Some(format!(
                "c-stub-object:{}:{index}",
                self.packages.get_package(*package).fqn
            )),
            BuildProduct::CStubLibrary { package } => Some(format!(
                "c-stub-library:{}",
                self.packages.get_package(*package).fqn
            )),
            BuildProduct::RuntimeLib => Some("native-runtime-library".to_owned()),
            _ => None,
        }
    }

    /// **For debug use only.** Prints debug information about a lowered action,
    /// the n2 build it's mapped into, and its input and output files.
    #[doc(hidden)]
    fn debug_print_command_and_files(&mut self, action: BuildActionId, build: &Build) {
        if log::log_enabled!(log::Level::Debug) {
            let in_files = build
                .ins
                .ids
                .iter()
                .map(|id| {
                    &self
                        .graph
                        .files
                        .by_id
                        .lookup(*id)
                        .expect("Input file should exist")
                        .name
                })
                .collect::<Vec<_>>();
            let out_files = build
                .outs
                .ids
                .iter()
                .map(|id| {
                    &self
                        .graph
                        .files
                        .by_id
                        .lookup(*id)
                        .expect("Output file should exist")
                        .name
                })
                .collect::<Vec<_>>();

            debug!(
                "lowered: {:?}\n into {:?};\n ins: {:?};\n outs: {:?}",
                action, build.cmdline, in_files, out_files
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::canonicalize_dependency_command_args;

    #[test]
    fn canonical_build_package_args_understand_compiler_path_forms() {
        let paths = vec![
            ("/cache/module/src/lib.mbt".to_owned(), "$source".to_owned()),
            ("/target/dependency.mi".to_owned(), "$dependency".to_owned()),
            ("/cache/module/src".to_owned(), "$package-root".to_owned()),
        ];
        let args = [
            "build-package",
            "/cache/module/src/lib.mbt",
            "-i",
            "/target/dependency.mi:alias",
            "-i",
            "/target/dependency.mi:/target/dependency.mi",
            "-pkg-sources",
            "example/pkg:/cache/module/src",
        ]
        .map(str::to_owned);

        assert_eq!(
            canonicalize_dependency_command_args(&args, &paths)
                .expect("known compiler path forms should canonicalize"),
            [
                "build-package",
                "$source",
                "-i",
                "$dependency:alias",
                "-i",
                "$dependency:$dependency",
                "-pkg-sources",
                "example/pkg:$package-root",
            ]
        );
    }

    #[test]
    fn canonical_build_package_args_reject_unknown_compound_paths() {
        let paths = vec![("/target/output.core".to_owned(), "$output".to_owned())];
        let args = ["build-package", "--future=/target/output.core"].map(str::to_owned);

        assert!(canonicalize_dependency_command_args(&args, &paths).is_none());
    }
}
