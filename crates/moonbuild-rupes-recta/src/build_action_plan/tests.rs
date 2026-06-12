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

use std::path::PathBuf;

use slotmap::KeyData;

use crate::{
    build_plan::{BuildPlan, BuildTargetInfo, FileDependencyKind, PlanArtifactNeed, PrebuildInfo},
    model::{BuildPlanNode, PackageId, TargetKind},
};

use super::{BuildAction, PlannedArtifact};

fn package_id(raw: u64) -> PackageId {
    PackageId::from(KeyData::from_ffi(raw))
}

fn target_info() -> BuildTargetInfo {
    BuildTargetInfo {
        regular_files: Vec::new(),
        mbtp_files: Vec::new(),
        whitebox_files: Vec::new(),
        doctest_files: Vec::new(),
        warn_list: None,
        specified_no_mi: false,
        patch_file: None,
        why3_config: None,
        check_mi_against: None,
        value_tracing: false,
    }
}

#[test]
fn check_exposes_package_interface() {
    let package = package_id(1);
    let target = package.build_target(TargetKind::Source);
    let node = BuildPlanNode::Check(target);
    let mut plan = BuildPlan::default();
    plan.test_add_node(node);
    plan.test_insert_build_target_info(target, target_info());

    let action_plan = plan.build_action_plan();
    let producer = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(producer),
        vec![PlannedArtifact::PackageInterface { producer, target }]
    );
}

#[test]
fn build_core_exposes_core_and_interface_when_it_emits_mi() {
    let package = package_id(1);
    let target = package.build_target(TargetKind::Source);
    let node = BuildPlanNode::BuildCore(target);
    let mut plan = BuildPlan::default();
    plan.test_add_node(node);
    plan.test_insert_build_target_info(target, target_info());

    let action_plan = plan.build_action_plan();
    let producer = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(producer),
        vec![
            PlannedArtifact::PackageInterface { producer, target },
            PlannedArtifact::PackageCoreIr { producer, target },
        ]
    );
}

#[test]
fn build_core_omits_interface_when_mi_is_disabled() {
    let package = package_id(1);
    let target = package.build_target(TargetKind::Source);
    let node = BuildPlanNode::BuildCore(target);
    let mut plan = BuildPlan::default();
    plan.test_add_node(node);
    let mut info = target_info();
    info.specified_no_mi = true;
    plan.test_insert_build_target_info(target, info);

    let action_plan = plan.build_action_plan();
    let producer = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(producer),
        vec![PlannedArtifact::PackageCoreIr { producer, target }]
    );
}

#[test]
fn make_executable_action_allows_non_native_alias_without_info() {
    let target = package_id(1).build_target(TargetKind::Source);
    let node = BuildPlanNode::MakeExecutable(target);
    let mut plan = BuildPlan::default();
    plan.test_add_node(node);

    let action_plan = plan.build_action_plan();
    let producer = action_plan.id_for_node(node);

    assert!(matches!(
        action_plan.action(producer),
        BuildAction::MakeExecutable { target: actual, info: None } if actual == target
    ));
    assert_eq!(
        action_plan.output_artifacts(producer),
        vec![PlannedArtifact::Executable { producer, target }]
    );
}

#[test]
fn check_interface_dependency_uses_selected_check_producer() {
    let dependency = package_id(1).build_target(TargetKind::Source);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let dependency_node = BuildPlanNode::Check(dependency);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let mut plan = BuildPlan::default();
    plan.test_add_edge(
        consumer_node,
        dependency_node,
        FileDependencyKind::Artifacts(PlanArtifactNeed::Interface),
    );

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_id = action_plan.id_for_node(dependency_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![PlannedArtifact::PackageInterface {
            producer: dependency_id,
            target: dependency,
        }]
    );
}

#[test]
fn build_core_dependency_can_track_interface_and_core_ir() {
    let dependency = package_id(1).build_target(TargetKind::Source);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let dependency_node = BuildPlanNode::BuildCore(dependency);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let mut plan = BuildPlan::default();
    plan.test_add_edge(
        consumer_node,
        dependency_node,
        FileDependencyKind::Artifacts(PlanArtifactNeed::InterfaceAndCoreIr),
    );
    plan.test_insert_build_target_info(dependency, target_info());

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_id = action_plan.id_for_node(dependency_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![
            PlannedArtifact::PackageInterface {
                producer: dependency_id,
                target: dependency,
            },
            PlannedArtifact::PackageCoreIr {
                producer: dependency_id,
                target: dependency,
            },
        ]
    );
}

#[test]
fn generate_test_info_dependency_can_select_driver_only() {
    let test_target = package_id(1).build_target(TargetKind::WhiteboxTest);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let test_info_node = BuildPlanNode::GenerateTestInfo(test_target);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let mut plan = BuildPlan::default();
    plan.test_add_edge(
        consumer_node,
        test_info_node,
        FileDependencyKind::GenerateTestInfo { meta: false },
    );

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let producer = action_plan.id_for_node(test_info_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![PlannedArtifact::GeneratedTestDriver {
            producer,
            target: test_target,
        }]
    );
}

#[test]
fn generate_test_info_dependency_can_select_driver_and_metadata() {
    let test_target = package_id(1).build_target(TargetKind::WhiteboxTest);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let test_info_node = BuildPlanNode::GenerateTestInfo(test_target);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let mut plan = BuildPlan::default();
    plan.test_add_edge(
        consumer_node,
        test_info_node,
        FileDependencyKind::GenerateTestInfo { meta: true },
    );

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let producer = action_plan.id_for_node(test_info_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![
            PlannedArtifact::GeneratedTestDriver {
                producer,
                target: test_target,
            },
            PlannedArtifact::GeneratedTestMetadata {
                producer,
                target: test_target,
            },
        ]
    );
}

#[test]
fn run_prebuild_exposes_resolved_outputs_as_known_paths() {
    let package = package_id(1);
    let node = BuildPlanNode::RunPrebuild(package, 0);
    let output = PathBuf::from("generated/out.mbt");
    let mut plan = BuildPlan::default();
    plan.test_add_node(node);
    plan.test_insert_prebuild_info(
        package,
        vec![Some(PrebuildInfo {
            resolved_inputs: Vec::new(),
            resolved_outputs: vec![output.clone()],
            cwd: PathBuf::from("."),
            command: "generate".to_string(),
        })],
    );

    let action_plan = plan.build_action_plan();
    let producer = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(producer),
        vec![PlannedArtifact::KnownPath {
            producer,
            path: output,
        }]
    );
}

#[test]
fn c_stub_archive_dependency_exposes_object_inputs() {
    let package = package_id(1);
    let archive_node = BuildPlanNode::ArchiveOrLinkCStubs(package);
    let object_node = BuildPlanNode::BuildCStub(package, 0);
    let mut plan = BuildPlan::default();
    plan.test_add_edge(archive_node, object_node, FileDependencyKind::AllFiles);

    let action_plan = plan.build_action_plan();
    let archive_id = action_plan.id_for_node(archive_node);
    let object_id = action_plan.id_for_node(object_node);

    assert_eq!(
        action_plan.dependency_artifacts(archive_id),
        vec![PlannedArtifact::CStubObject {
            producer: object_id,
            package,
            index: 0,
        }]
    );
}
