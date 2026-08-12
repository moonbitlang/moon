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

//! Executor-neutral actions produced by lowering a semantic Build Plan.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use indexmap::IndexMap;

use crate::{build_plan::ArtifactKey, pkg_name::OptionalPackageFQNWithSource};

mod n2_adapter;

pub use n2_adapter::{CommandArgMap, N2AdapterError};

/// Process-local identity of one concrete execution action.
///
/// This is an arena handle, not the persistent action digest used by the build
/// cache. It is meaningful only within its owning [`ExecutionPlan`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionId(pub(crate) usize);

/// Process-local identity of one declared physical output.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputId(pub(crate) usize);

/// A concrete file observation that is not produced by an action in this plan.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExternalInput {
    /// One regular file observed by the action.
    File(PathBuf),
    /// The recursive `.mi` tree observed through `moonc -std-path`.
    StandardLibraryInterfaces(PathBuf),
}

impl ExternalInput {
    pub fn path(&self) -> &Path {
        match self {
            Self::File(path) | Self::StandardLibraryInterfaces(path) => path,
        }
    }

    pub(crate) fn n2_path(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::StandardLibraryInterfaces(_) => None,
        }
    }
}

/// A response file selected while lowering a command.
#[derive(Debug, Clone)]
pub struct LoweredResponseFile {
    pub path: PathBuf,
    pub content: String,
}

/// How the process command should be transported to an executor.
#[derive(Debug, Clone)]
pub enum LoweredCommandExecution {
    Inline(String),
    ResponseFile {
        command: String,
        file: LoweredResponseFile,
    },
}

/// A concrete process command and its selected process transport.
///
/// `args` retains the logical form for metadata consumers. `execution` records
/// whether the same command is sent inline or through a response file.
#[derive(Debug, Clone)]
pub struct LoweredCommand {
    pub(crate) args: Vec<String>,
    pub(crate) execution: LoweredCommandExecution,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) env: Vec<(String, String)>,
}

impl From<Vec<String>> for LoweredCommand {
    fn from(args: Vec<String>) -> Self {
        let command = moonutil::shlex::join_native(args.iter().map(String::as_str));
        Self {
            args,
            execution: LoweredCommandExecution::Inline(command),
            cwd: None,
            env: Vec::new(),
        }
    }
}

impl LoweredCommand {
    pub(crate) fn inline_command(&self) -> &str {
        let LoweredCommandExecution::Inline(command) = &self.execution else {
            unreachable!("a response-file command is already lowered")
        };
        command
    }

    pub(crate) fn with_response_file(mut self, command: String, file: LoweredResponseFile) -> Self {
        self.execution = LoweredCommandExecution::ResponseFile { command, file };
        self
    }

    pub(crate) fn executable(&self) -> Option<&Path> {
        self.args.first().map(Path::new)
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn execution(&self) -> &LoweredCommandExecution {
        &self.execution
    }

    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    pub(crate) fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    pub(crate) fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env.extend(env);
        self
    }
}

/// One physical file or directory declared to execution adapters.
///
/// A declared output may realize a semantic Build Artifact, but execution-only
/// outputs such as a dSYM bundle deliberately have no `ArtifactKey`.
#[derive(Debug)]
pub struct DeclaredOutput {
    producer: ActionId,
    path: PathBuf,
    artifact: Option<ArtifactKey>,
}

impl DeclaredOutput {
    pub fn producer(&self) -> ActionId {
        self.producer
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn artifact(&self) -> Option<&ArtifactKey> {
        self.artifact.as_ref()
    }
}

/// One concrete action after command construction and path realization.
#[derive(Debug)]
pub struct ExecutionAction {
    artifact_inputs: Vec<ArtifactKey>,
    external_inputs: Vec<ExternalInput>,
    artifact_outputs: Vec<ArtifactKey>,
    outputs: Vec<OutputId>,
    command: LoweredCommand,
    cache_eligible: bool,
    fileloc: String,
    description: String,
    can_dirty_on_output: bool,
    error_package: OptionalPackageFQNWithSource,
}

impl ExecutionAction {
    pub fn artifact_inputs(&self) -> &[ArtifactKey] {
        &self.artifact_inputs
    }

    pub fn external_inputs(&self) -> &[ExternalInput] {
        &self.external_inputs
    }

    pub fn artifact_outputs(&self) -> &[ArtifactKey] {
        &self.artifact_outputs
    }

    pub fn outputs(&self) -> &[OutputId] {
        &self.outputs
    }

    pub fn command(&self) -> &LoweredCommand {
        &self.command
    }

    /// Whether lowering has a complete enough execution model to permit reuse.
    pub fn is_cache_eligible(&self) -> bool {
        self.cache_eligible
    }

    pub fn can_dirty_on_output(&self) -> bool {
        self.can_dirty_on_output
    }

    pub fn fileloc(&self) -> &str {
        &self.fileloc
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn error_package(&self) -> &OptionalPackageFQNWithSource {
        &self.error_package
    }
}

#[derive(Debug)]
struct ArtifactRealization {
    producer: ActionId,
    outputs: Vec<OutputId>,
}

/// The executor-neutral graph shared by n2, dry-run, and cache consumers.
///
/// Actions depend on semantic artifacts, while declared physical outputs have
/// independent `OutputId` values. This lets execution adapters track physical
/// outputs without promoting every file or directory into `ArtifactKey`.
#[derive(Debug, Default)]
pub struct ExecutionPlan {
    actions: Vec<ExecutionAction>,
    outputs: Vec<DeclaredOutput>,
    artifacts: HashMap<ArtifactKey, ArtifactRealization>,
    requested_artifacts: IndexMap<ArtifactKey, Vec<OutputId>>,
}

impl ExecutionPlan {
    pub fn action_ids(&self) -> impl Iterator<Item = ActionId> + '_ {
        (0..self.actions.len()).map(ActionId)
    }

    pub fn action(&self, id: ActionId) -> &ExecutionAction {
        &self.actions[id.0]
    }

    pub fn output(&self, id: OutputId) -> &DeclaredOutput {
        &self.outputs[id.0]
    }

    pub fn artifact_producer(&self, artifact: &ArtifactKey) -> ActionId {
        self.artifacts
            .get(artifact)
            .unwrap_or_else(|| panic!("execution artifact {artifact:?} has no provider"))
            .producer
    }

    pub fn artifact_outputs(&self, artifact: &ArtifactKey) -> &[OutputId] {
        &self
            .artifacts
            .get(artifact)
            .unwrap_or_else(|| panic!("execution artifact {artifact:?} has no realization"))
            .outputs
    }

    pub fn artifact_paths(&self, artifact: &ArtifactKey) -> Vec<&Path> {
        self.artifact_outputs(artifact)
            .iter()
            .map(|output| self.output(*output).path())
            .collect()
    }

    pub fn requested_artifact_paths(&self) -> impl Iterator<Item = (&ArtifactKey, Vec<PathBuf>)> {
        self.requested_artifacts.iter().map(|(artifact, outputs)| {
            (
                artifact,
                outputs
                    .iter()
                    .map(|output| self.output(*output).path.clone())
                    .collect(),
            )
        })
    }

    pub fn requested_artifacts(&self) -> impl Iterator<Item = (&ArtifactKey, &[OutputId])> {
        self.requested_artifacts
            .iter()
            .map(|(artifact, outputs)| (artifact, outputs.as_slice()))
    }

    pub fn to_n2_graph(
        &self,
        actions: impl IntoIterator<Item = ActionId>,
    ) -> Result<(n2::graph::Graph, CommandArgMap), N2AdapterError> {
        n2_adapter::to_n2_graph(self, actions)
    }

    pub fn all_to_n2_graph(&self) -> Result<(n2::graph::Graph, CommandArgMap), N2AdapterError> {
        self.to_n2_graph(self.action_ids())
    }
}

pub(crate) struct ExecutionActionDraft {
    pub(crate) artifact_inputs: Vec<ArtifactKey>,
    pub(crate) semantic_outputs: Vec<(ArtifactKey, Vec<PathBuf>)>,
    pub(crate) declared_outputs: Vec<PathBuf>,
    pub(crate) external_inputs: Vec<ExternalInput>,
    pub(crate) command: LoweredCommand,
    pub(crate) cache_eligible: bool,
    pub(crate) fileloc: String,
    pub(crate) description: String,
    pub(crate) can_dirty_on_output: bool,
    pub(crate) error_package: OptionalPackageFQNWithSource,
}

impl ExecutionActionDraft {
    #[cfg(test)]
    pub(crate) fn is_cache_eligible(&self) -> bool {
        self.cache_eligible
    }
}

#[derive(Default)]
pub(crate) struct ExecutionPlanBuilder {
    plan: ExecutionPlan,
    output_by_path: HashMap<PathBuf, OutputId>,
}

impl ExecutionPlanBuilder {
    pub(crate) fn add_action(&mut self, draft: ExecutionActionDraft) -> ActionId {
        let action = ActionId(self.plan.actions.len());
        let mut output_ids = Vec::new();
        let mut artifact_outputs = Vec::new();

        for (artifact, paths) in draft.semantic_outputs {
            let outputs = paths
                .into_iter()
                .map(|path| self.add_output(action, path, Some(artifact.clone())))
                .collect::<Vec<_>>();
            let previous = self.plan.artifacts.insert(
                artifact.clone(),
                ArtifactRealization {
                    producer: action,
                    outputs: outputs.clone(),
                },
            );
            assert!(
                previous.is_none(),
                "execution artifact has multiple providers: {artifact:?}"
            );
            artifact_outputs.push(artifact);
            output_ids.extend(outputs);
        }
        output_ids.extend(
            draft
                .declared_outputs
                .into_iter()
                .map(|path| self.add_output(action, path, None)),
        );

        self.plan.actions.push(ExecutionAction {
            artifact_inputs: draft.artifact_inputs,
            external_inputs: draft.external_inputs,
            artifact_outputs,
            outputs: output_ids,
            command: draft.command,
            cache_eligible: draft.cache_eligible,
            fileloc: draft.fileloc,
            description: draft.description,
            can_dirty_on_output: draft.can_dirty_on_output,
            error_package: draft.error_package,
        });
        action
    }

    fn add_output(
        &mut self,
        producer: ActionId,
        path: PathBuf,
        artifact: Option<ArtifactKey>,
    ) -> OutputId {
        assert!(
            !self.output_by_path.contains_key(&path),
            "declared execution output has multiple producers: {}",
            path.display()
        );
        let output = OutputId(self.plan.outputs.len());
        self.plan.outputs.push(DeclaredOutput {
            producer,
            path: path.clone(),
            artifact,
        });
        self.output_by_path.insert(path, output);
        output
    }

    pub(crate) fn finish(
        mut self,
        requested_artifacts: impl IntoIterator<Item = ArtifactKey>,
    ) -> ExecutionPlan {
        for action in &self.plan.actions {
            for artifact in &action.artifact_inputs {
                assert!(
                    self.plan.artifacts.contains_key(artifact),
                    "execution action requires artifact without a provider: {artifact:?}"
                );
            }
        }

        let mut seen = HashSet::new();
        for artifact in requested_artifacts {
            if !seen.insert(artifact.clone()) {
                continue;
            }
            let outputs = self
                .plan
                .artifacts
                .get(&artifact)
                .unwrap_or_else(|| panic!("requested artifact has no realization: {artifact:?}"))
                .outputs
                .clone();
            self.plan.requested_artifacts.insert(artifact, outputs);
        }
        self.plan
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::pkg_name::PackageFQN;

    use super::*;

    fn draft(
        artifact_inputs: Vec<ArtifactKey>,
        semantic_outputs: Vec<(ArtifactKey, Vec<PathBuf>)>,
        declared_outputs: Vec<PathBuf>,
        command: &str,
    ) -> ExecutionActionDraft {
        ExecutionActionDraft {
            artifact_inputs,
            semantic_outputs,
            declared_outputs,
            external_inputs: Vec::new(),
            command: LoweredCommand::from(vec![command.to_string()]),
            cache_eligible: true,
            fileloc: command.to_string(),
            description: command.to_string(),
            can_dirty_on_output: false,
            error_package: None::<PackageFQN>.into(),
        }
    }

    fn producer_and_consumer_plan() -> (ExecutionPlan, ActionId, ActionId) {
        let mut builder = ExecutionPlanBuilder::default();
        let producer = builder.add_action(draft(
            Vec::new(),
            vec![(
                ArtifactKey::RuntimeLibrary,
                vec![PathBuf::from("build/libmoonbitrun.a")],
            )],
            Vec::new(),
            "archive-runtime",
        ));
        let consumer = builder.add_action(draft(
            vec![ArtifactKey::RuntimeLibrary],
            Vec::new(),
            vec![PathBuf::from("build/app.dSYM")],
            "generate-debug-symbols",
        ));
        let plan = builder.finish([ArtifactKey::RuntimeLibrary]);
        (plan, producer, consumer)
    }

    #[test]
    fn artifact_edges_and_physical_only_outputs_have_distinct_identity() {
        let (plan, producer, consumer) = producer_and_consumer_plan();

        assert_eq!(
            plan.artifact_producer(&ArtifactKey::RuntimeLibrary),
            producer
        );
        assert_eq!(
            plan.artifact_paths(&ArtifactKey::RuntimeLibrary),
            vec![Path::new("build/libmoonbitrun.a")]
        );

        let debug_output = plan.output(plan.action(consumer).outputs()[0]);
        assert_eq!(debug_output.producer(), consumer);
        assert_eq!(debug_output.path(), Path::new("build/app.dSYM"));
        assert_eq!(debug_output.artifact(), None);
    }

    #[test]
    fn selected_consumer_keeps_an_omitted_producer_artifact_as_an_input() {
        let (plan, _, consumer) = producer_and_consumer_plan();
        let (graph, _) = plan
            .to_n2_graph([consumer])
            .expect("selected execution action should adapt to n2");

        assert_eq!(graph.builds.iter().count(), 1);
        let build = graph
            .builds
            .iter()
            .next()
            .expect("consumer build should be present");
        assert_eq!(
            build
                .ins
                .ids
                .iter()
                .map(|id| graph.files.by_id[*id].name.as_str())
                .collect::<Vec<_>>(),
            vec!["build/libmoonbitrun.a"]
        );
        let input = build.ins.ids[0];
        assert!(
            graph.files.by_id[input].input.is_none(),
            "the omitted producer is expected to run in an earlier execution phase"
        );
    }
}
