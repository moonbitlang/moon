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
    build_plan::{ArtifactKey, BuildPlan, BuildTargetInfo, LinkCoreInfo, PrebuildInfo},
    model::{BuildPlanNode, PackageId, TargetKind},
};

use super::BuildAction;

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
        proof_prelude: PathBuf::from("proof-prelude"),
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
    plan.test_provide_artifact(
        node,
        ArtifactKey::CheckMi {
            package,
            target_kind: target.kind,
        },
    );

    let action_plan = plan.build_action_plan();
    let action_id = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(action_id),
        vec![ArtifactKey::CheckMi {
            package,
            target_kind: target.kind,
        }]
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
    plan.test_provide_artifact(
        node,
        ArtifactKey::BuildMi {
            package,
            target_kind: target.kind,
        },
    );
    plan.test_provide_artifact(
        node,
        ArtifactKey::CoreIr {
            package,
            target_kind: target.kind,
        },
    );

    let action_plan = plan.build_action_plan();
    let action_id = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(action_id),
        vec![
            ArtifactKey::BuildMi {
                package,
                target_kind: target.kind,
            },
            ArtifactKey::CoreIr {
                package,
                target_kind: target.kind,
            },
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
    plan.test_provide_artifact(
        node,
        ArtifactKey::CoreIr {
            package,
            target_kind: target.kind,
        },
    );

    let action_plan = plan.build_action_plan();
    let action_id = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(action_id),
        vec![ArtifactKey::CoreIr {
            package,
            target_kind: target.kind,
        }]
    );
}

#[test]
fn link_core_can_provide_a_non_native_executable() {
    let target = package_id(1).build_target(TargetKind::Source);
    let node = BuildPlanNode::LinkCore(target);
    let mut plan = BuildPlan::default();
    plan.test_add_node(node);
    plan.test_insert_link_core_info(
        target,
        LinkCoreInfo {
            linked_order: Vec::new(),
            abort_overridden: false,
        },
    );
    plan.test_provide_artifact(
        node,
        ArtifactKey::Executable {
            package: target.package,
            target_kind: target.kind,
        },
    );

    let action_plan = plan.build_action_plan();
    let action_id = action_plan.id_for_node(node);

    assert!(matches!(
        action_plan.action(action_id),
        BuildAction::LinkCore { target: actual, .. } if actual == target
    ));
    assert_eq!(
        action_plan.output_artifacts(action_id),
        vec![ArtifactKey::Executable {
            package: target.package,
            target_kind: target.kind,
        }]
    );
}

#[test]
fn check_interface_dependency_uses_selected_check_action() {
    let dependency = package_id(1).build_target(TargetKind::Source);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let dependency_node = BuildPlanNode::Check(dependency);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let check_mi = ArtifactKey::CheckMi {
        package: dependency.package,
        target_kind: dependency.kind,
    };
    let mut plan = BuildPlan::default();
    plan.test_require_artifact(consumer_node, check_mi.clone());
    plan.test_provide_artifact(dependency_node, check_mi.clone());

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_id = action_plan.id_for_node(dependency_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![(dependency_id, check_mi)]
    );
}

#[test]
fn build_core_dependency_can_track_interface_and_core_ir() {
    let dependency = package_id(1).build_target(TargetKind::Source);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let dependency_node = BuildPlanNode::BuildCore(dependency);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let build_mi = ArtifactKey::BuildMi {
        package: dependency.package,
        target_kind: dependency.kind,
    };
    let core_ir = ArtifactKey::CoreIr {
        package: dependency.package,
        target_kind: dependency.kind,
    };
    let mut plan = BuildPlan::default();
    plan.test_require_artifact(consumer_node, build_mi.clone());
    plan.test_require_artifact(consumer_node, core_ir.clone());
    plan.test_provide_artifact(dependency_node, build_mi.clone());
    plan.test_provide_artifact(dependency_node, core_ir.clone());

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_id = action_plan.id_for_node(dependency_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![(dependency_id, build_mi), (dependency_id, core_ir),]
    );
}

#[test]
fn artifact_dependencies_deduplicate_provider_action() {
    let dependency = package_id(1).build_target(TargetKind::Source);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let dependency_node = BuildPlanNode::BuildCore(dependency);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let build_mi = ArtifactKey::BuildMi {
        package: dependency.package,
        target_kind: dependency.kind,
    };
    let core_ir = ArtifactKey::CoreIr {
        package: dependency.package,
        target_kind: dependency.kind,
    };
    let mut plan = BuildPlan::default();
    plan.test_require_artifact(consumer_node, build_mi.clone());
    plan.test_require_artifact(consumer_node, core_ir.clone());
    plan.test_provide_artifact(dependency_node, build_mi);
    plan.test_provide_artifact(dependency_node, core_ir);

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_id = action_plan.id_for_node(dependency_node);

    assert_eq!(
        action_plan
            .dependency_action_ids(consumer_id)
            .collect::<Vec<_>>(),
        vec![dependency_id]
    );
}

#[test]
fn generate_test_info_dependency_can_select_driver_only() {
    let test_target = package_id(1).build_target(TargetKind::WhiteboxTest);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let test_info_node = BuildPlanNode::GenerateTestInfo(test_target);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let mut plan = BuildPlan::default();
    let driver = ArtifactKey::GeneratedTestDriver {
        package: test_target.package,
        target_kind: test_target.kind,
    };
    plan.test_require_artifact(consumer_node, driver.clone());
    plan.test_provide_artifact(test_info_node, driver.clone());

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_action = action_plan.id_for_node(test_info_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![(dependency_action, driver,)]
    );
}

#[test]
fn generate_test_info_dependency_can_select_driver_and_metadata() {
    let test_target = package_id(1).build_target(TargetKind::WhiteboxTest);
    let consumer = package_id(2).build_target(TargetKind::Source);
    let test_info_node = BuildPlanNode::GenerateTestInfo(test_target);
    let consumer_node = BuildPlanNode::BuildCore(consumer);
    let mut plan = BuildPlan::default();
    let driver = ArtifactKey::GeneratedTestDriver {
        package: test_target.package,
        target_kind: test_target.kind,
    };
    let metadata = ArtifactKey::GeneratedTestMetadata {
        package: test_target.package,
        target_kind: test_target.kind,
    };
    plan.test_require_artifact(consumer_node, driver.clone());
    plan.test_require_artifact(consumer_node, metadata.clone());
    plan.test_provide_artifact(test_info_node, driver.clone());
    plan.test_provide_artifact(test_info_node, metadata.clone());

    let action_plan = plan.build_action_plan();
    let consumer_id = action_plan.id_for_node(consumer_node);
    let dependency_action = action_plan.id_for_node(test_info_node);

    assert_eq!(
        action_plan.dependency_artifacts(consumer_id),
        vec![(dependency_action, driver,), (dependency_action, metadata,),]
    );
}

#[test]
fn run_prebuild_exposes_resolved_output_paths() {
    let package = package_id(1);
    let node = BuildPlanNode::RunPrebuild(package, 0);
    let output = PathBuf::from("generated/out.mbt");
    let mut plan = BuildPlan::default();
    plan.test_insert_prebuild_info(
        package,
        vec![Some(PrebuildInfo {
            resolved_inputs: Vec::new(),
            resolved_outputs: vec![output.clone()],
            cwd: PathBuf::from("."),
            command: "generate".to_string(),
        })],
    );
    plan.test_provide_artifact(
        node,
        ArtifactKey::PrebuildOutput {
            package,
            path: output.clone(),
        },
    );

    let action_plan = plan.build_action_plan();
    let action_id = action_plan.id_for_node(node);

    assert_eq!(
        action_plan.output_artifacts(action_id),
        vec![ArtifactKey::PrebuildOutput {
            package,
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
    let object = ArtifactKey::CStubObject {
        package,
        source: PathBuf::from("stub.c"),
    };
    plan.test_require_artifact(archive_node, object.clone());
    plan.test_provide_artifact(object_node, object.clone());

    let action_plan = plan.build_action_plan();
    let archive_id = action_plan.id_for_node(archive_node);
    let object_id = action_plan.id_for_node(object_node);

    assert_eq!(
        action_plan.dependency_artifacts(archive_id),
        vec![(object_id, object)]
    );
}

#[test]
fn runtime_archive_dependency_exposes_object_inputs() {
    let archive_node = BuildPlanNode::BuildRuntimeLib;
    let object_node = BuildPlanNode::BuildRuntimeObject(0);
    let mut plan = BuildPlan::default();
    let object = ArtifactKey::RuntimeObject {
        source: PathBuf::from("runtime/runtime.c"),
    };
    plan.test_require_artifact(archive_node, object.clone());
    plan.test_provide_artifact(object_node, object.clone());

    let action_plan = plan.build_action_plan();
    let archive_id = action_plan.id_for_node(archive_node);
    let object_id = action_plan.id_for_node(object_node);

    assert_eq!(
        action_plan.dependency_artifacts(archive_id),
        vec![(object_id, object)]
    );
}
