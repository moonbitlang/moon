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

//! Realized paths for logical build products.

use std::{collections::HashMap, path::PathBuf};

use crate::{
    ResolveOutput,
    build_action_plan::{BuildAction, BuildActionPlan, PlannedArtifact},
    build_lower::{
        BuildOptions,
        artifact::{LegacyLayout, MiPathResult},
    },
    model::TargetKind,
};

pub(crate) struct ProductTable {
    paths_by_artifact: HashMap<PlannedArtifact, Vec<PathBuf>>,
}

impl ProductTable {
    pub(crate) fn new(
        layout: &LegacyLayout,
        resolve_output: &ResolveOutput,
        plan: &BuildActionPlan<'_>,
        opt: &BuildOptions,
    ) -> Self {
        let resolver = ProductResolver {
            layout,
            resolve_output,
            plan,
            opt,
        };
        let mut paths_by_artifact = HashMap::new();
        for action in plan.action_ids() {
            for artifact in plan.output_artifacts(action) {
                let paths = resolver.resolve(&artifact);
                paths_by_artifact.insert(artifact, paths);
            }
        }
        Self { paths_by_artifact }
    }

    pub(crate) fn paths(&self, artifact: &PlannedArtifact) -> &[PathBuf] {
        self.paths_by_artifact
            .get(artifact)
            .unwrap_or_else(|| panic!("planned artifact should be realized: {artifact:?}"))
    }
}

struct ProductResolver<'a, 'b> {
    layout: &'a LegacyLayout,
    resolve_output: &'a ResolveOutput,
    plan: &'a BuildActionPlan<'b>,
    opt: &'a BuildOptions,
}

impl ProductResolver<'_, '_> {
    fn resolve(&self, artifact: &PlannedArtifact) -> Vec<PathBuf> {
        let packages = &self.resolve_output.pkg_dirs;
        let modules = &self.resolve_output.module_rel;
        match artifact {
            PlannedArtifact::PackageInterface { producer, target } => {
                self.package_interface(*producer, *target)
            }
            PlannedArtifact::PackageCoreIr { target, .. } => {
                vec![self.layout.core_of_build_target(
                    packages,
                    target,
                    self.opt.target_backend.into(),
                )]
            }
            PlannedArtifact::ProofInterface { producer, target } => {
                match self.plan.action(*producer) {
                    BuildAction::EmitProof { .. } => {
                        vec![self.layout.emit_proof_mi_path(packages, target)]
                    }
                    BuildAction::Prove { .. } => vec![self.layout.prove_mi_path(packages, target)],
                    _ => unreachable!("proof interface producer should be a proof action"),
                }
            }
            PlannedArtifact::ProofWhyml { producer, target } => match self.plan.action(*producer) {
                BuildAction::EmitProof { .. } => {
                    vec![self.layout.emit_proof_whyml_path(packages, target)]
                }
                BuildAction::Prove { .. } => vec![self.layout.prove_whyml_path(packages, target)],
                _ => unreachable!("proof whyml producer should be a proof action"),
            },
            PlannedArtifact::ProofReport { target, .. } => {
                vec![self.layout.prove_report_path(packages, target)]
            }
            PlannedArtifact::CStubObject { package, index, .. } => {
                let pkg = packages.get_package(*package);
                let file_name = &pkg.c_stub_files[*index as usize];
                vec![
                    self.layout.c_stub_object_path(
                        packages,
                        *package,
                        file_name
                            .file_stem()
                            .expect("c stub file should have a file name"),
                        self.opt.target_backend.into(),
                        self.opt.os(),
                    ),
                ]
            }
            PlannedArtifact::CStubLibrary { package, .. } => {
                vec![self.opt.selected_backend.c_stub_library_path(
                    self.layout,
                    packages,
                    *package,
                    self.opt.target_backend.into(),
                )]
            }
            PlannedArtifact::LinkedCore { target, .. } => {
                vec![
                    self.opt
                        .selected_backend
                        .linked_core_path(self.layout, packages, target),
                ]
            }
            PlannedArtifact::Executable { target, .. } => {
                vec![
                    self.opt
                        .selected_backend
                        .executable_path(self.layout, packages, target),
                ]
            }
            PlannedArtifact::GeneratedTestDriver { target, .. } => {
                vec![self.layout.generated_test_driver(
                    packages,
                    target,
                    self.opt.target_backend.into(),
                )]
            }
            PlannedArtifact::GeneratedTestMetadata { target, .. } => {
                vec![self.layout.generated_test_driver_metadata(
                    packages,
                    target,
                    self.opt.target_backend.into(),
                )]
            }
            PlannedArtifact::BundleResult { module, .. } => {
                let module_name = modules.module_source(*module);
                vec![
                    self.layout
                        .bundle_result_path(self.opt.target_backend.into(), module_name.name()),
                ]
            }
            PlannedArtifact::RuntimeLib { .. } => {
                vec![self.opt.selected_backend.runtime_path(self.layout)]
            }
            PlannedArtifact::GeneratedMbti { target, .. } => vec![self.layout.generated_mbti_path(
                packages,
                target,
                self.opt.target_backend.into(),
            )],
            PlannedArtifact::DocsDir { .. } => vec![self.layout.doc_dir()],
            PlannedArtifact::VirtualPackageInterface { package, .. } => {
                let target = package.build_target(TargetKind::Source);
                vec![self.layout.mi_of_build_target(
                    packages,
                    &target,
                    self.opt.target_backend.into(),
                )]
            }
            PlannedArtifact::MoonLexGeneratedSource { package, index, .. } => {
                let pkg_info = packages.get_package(*package);
                let mbtlex_file = &pkg_info.mbt_lex_files[*index as usize];
                vec![mbtlex_file.with_extension("mbt")]
            }
            PlannedArtifact::MoonYaccGeneratedSource { package, index, .. } => {
                let pkg_info = packages.get_package(*package);
                let mbtyacc_file = &pkg_info.mbt_yacc_files[*index as usize];
                vec![mbtyacc_file.with_extension("mbt")]
            }
            PlannedArtifact::KnownPath { path, .. } => vec![path.clone()],
        }
    }

    fn package_interface(
        &self,
        producer: crate::build_action_plan::BuildActionId,
        target: crate::model::BuildTarget,
    ) -> Vec<PathBuf> {
        let packages = &self.resolve_output.pkg_dirs;
        match self.plan.action(producer) {
            BuildAction::Check { info, .. } if info.check_mi_against.is_some() => {
                match self.layout.mi_of_build_target_impl_virtual(
                    packages,
                    &target,
                    self.opt.target_backend.into(),
                ) {
                    MiPathResult::StdAbort(_) => Vec::new(),
                    MiPathResult::Std(p) => {
                        tracing::warn!(
                            "stdlib mi should not be needed for check as an implementation package: {:?}",
                            p
                        );
                        Vec::new()
                    }
                    MiPathResult::Regular(p) => vec![p],
                }
            }
            BuildAction::Check { .. } | BuildAction::BuildCore { .. } => {
                vec![self.layout.mi_of_build_target(
                    packages,
                    &target,
                    self.opt.target_backend.into(),
                )]
            }
            _ => unreachable!("package interface producer should be Check or BuildCore"),
        }
    }
}
