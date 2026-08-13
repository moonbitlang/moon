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

use crate::{build_plan::ArtifactKey, pkg_name::OptionalPackageFQNWithSource};

mod n2_adapter;

pub use n2_adapter::{CommandArgMap, N2AdapterError};

/// One n2 projection together with the process-local action provenance that
/// n2 itself does not retain.
pub struct N2Projection {
    graph: n2::graph::Graph,
    command_args_by_output: CommandArgMap,
    action_by_build: HashMap<n2::graph::BuildId, ActionId>,
}

impl N2Projection {
    pub fn graph(&self) -> &n2::graph::Graph {
        &self.graph
    }

    pub fn action_for_build(&self, build: n2::graph::BuildId) -> Option<ActionId> {
        self.action_by_build.get(&build).copied()
    }

    pub fn into_parts(self) -> (n2::graph::Graph, CommandArgMap) {
        (self.graph, self.command_args_by_output)
    }

    pub fn into_parts_with_actions(
        self,
    ) -> (
        n2::graph::Graph,
        CommandArgMap,
        HashMap<n2::graph::BuildId, ActionId>,
    ) {
        (
            self.graph,
            self.command_args_by_output,
            self.action_by_build,
        )
    }
}

/// Process-local identity of one concrete execution action.
///
/// This is an arena handle, not the persistent action digest used by the build
/// cache. It is meaningful only within its owning [`ExecutionPlan`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionId(pub(crate) usize);

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
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LoweredResponseFile {
    pub path: PathBuf,
    pub content: String,
}

/// How the process command should be transported to an executor.
#[derive(Debug, Clone, Eq, PartialEq)]
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
#[derive(Debug, Clone, Eq, PartialEq)]
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
/// outputs such as a dSYM bundle deliberately have no `ArtifactKey`. The path
/// is its execution identity; it does not replace this semantic annotation.
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionAction {
    inputs: Vec<PathBuf>,
    external_inputs: Vec<ExternalInput>,
    outputs: Vec<PathBuf>,
    command: LoweredCommand,
    cache_eligible: bool,
    fileloc: String,
    description: String,
    can_dirty_on_output: bool,
    error_package: OptionalPackageFQNWithSource,
}

impl ExecutionAction {
    pub(crate) fn new(
        inputs: Vec<PathBuf>,
        outputs: Vec<PathBuf>,
        command: LoweredCommand,
        fileloc: String,
        description: String,
    ) -> Self {
        Self {
            inputs,
            external_inputs: Vec::new(),
            outputs,
            command,
            cache_eligible: true,
            fileloc,
            description,
            can_dirty_on_output: false,
            error_package: None::<crate::pkg_name::PackageFQN>.into(),
        }
    }

    pub(crate) fn with_external_inputs(mut self, external_inputs: Vec<ExternalInput>) -> Self {
        self.external_inputs = external_inputs;
        self
    }

    pub(crate) fn with_cache_eligible(mut self, cache_eligible: bool) -> Self {
        self.cache_eligible = cache_eligible;
        self
    }

    pub(crate) fn with_can_dirty_on_output(mut self, can_dirty_on_output: bool) -> Self {
        self.can_dirty_on_output = can_dirty_on_output;
        self
    }

    pub(crate) fn with_error_package(
        mut self,
        error_package: OptionalPackageFQNWithSource,
    ) -> Self {
        self.error_package = error_package;
        self
    }

    pub fn inputs(&self) -> &[PathBuf] {
        &self.inputs
    }

    pub fn external_inputs(&self) -> &[ExternalInput] {
        &self.external_inputs
    }

    pub fn outputs(&self) -> &[PathBuf] {
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

/// The executor-neutral graph shared by n2, dry-run, and cache consumers.
///
/// Build Artifact requirements are realized as concrete input paths. Declared
/// outputs retain their producer and optional artifact annotation without
/// promoting every file or directory into `ArtifactKey`. Consumers that need
/// planning semantics can resolve an input path through
/// [`ExecutionPlan::declared_output`] instead of inferring meaning from its
/// filename.
#[derive(Clone, Debug, Default)]
pub struct ExecutionPlan {
    actions: Vec<ExecutionAction>,
    outputs: HashMap<PathBuf, DeclaredOutput>,
    requested_artifacts: Vec<(ArtifactKey, Vec<PathBuf>)>,
}

impl ExecutionPlan {
    pub fn action_ids(&self) -> impl Iterator<Item = ActionId> + '_ {
        (0..self.actions.len()).map(ActionId)
    }

    pub fn action(&self, id: ActionId) -> &ExecutionAction {
        &self.actions[id.0]
    }

    pub fn declared_output(&self, path: &Path) -> Option<&DeclaredOutput> {
        self.outputs.get(path)
    }

    pub fn requested_artifact_paths(&self) -> impl Iterator<Item = (&ArtifactKey, &[PathBuf])> {
        self.requested_artifacts
            .iter()
            .map(|(artifact, outputs)| (artifact, outputs.as_slice()))
    }

    /// Add an independently lowered plan and return its action IDs in this plan.
    ///
    /// Concrete output paths form the composition boundary. Plans may share an
    /// action only when every part of its execution behavior and every output
    /// annotation agree. A partial overlap or a different producer is rejected
    /// before the executor sees an ambiguous graph.
    pub fn merge(
        &mut self,
        other: &ExecutionPlan,
    ) -> Result<Vec<ActionId>, ExecutionPlanMergeError> {
        let mut action_ids = Vec::with_capacity(other.actions.len());

        for action in &other.actions {
            let existing_producers = action
                .outputs
                .iter()
                .filter_map(|path| self.outputs.get(path).map(DeclaredOutput::producer))
                .collect::<HashSet<_>>();

            let action_id = if existing_producers.is_empty() {
                let action_id = ActionId(self.actions.len());
                self.actions.push(action.clone());
                for path in &action.outputs {
                    let output = other
                        .outputs
                        .get(path)
                        .expect("execution action outputs should be declared");
                    self.outputs.insert(
                        path.clone(),
                        DeclaredOutput {
                            producer: action_id,
                            path: path.clone(),
                            artifact: output.artifact.clone(),
                        },
                    );
                }
                action_id
            } else {
                let Some(&existing) = existing_producers.iter().next() else {
                    unreachable!()
                };
                let outputs_match = existing_producers.len() == 1
                    && action.outputs.len() == self.actions[existing.0].outputs.len()
                    && action.outputs.iter().all(|path| {
                        self.outputs.get(path).is_some_and(|current| {
                            current.producer == existing
                                && other
                                    .outputs
                                    .get(path)
                                    .is_some_and(|incoming| current.artifact == incoming.artifact)
                        })
                    });
                if !outputs_match || self.actions[existing.0] != *action {
                    let path = action
                        .outputs
                        .iter()
                        .find(|path| self.outputs.contains_key(*path))
                        .expect("an existing producer was found")
                        .clone();
                    return Err(ExecutionPlanMergeError::ConflictingOutput { path });
                }
                existing
            };
            action_ids.push(action_id);
        }

        self.requested_artifacts
            .extend(other.requested_artifacts.iter().cloned());
        Ok(action_ids)
    }

    pub fn to_n2_graph(
        &self,
        actions: impl IntoIterator<Item = ActionId>,
    ) -> Result<(n2::graph::Graph, CommandArgMap), N2AdapterError> {
        self.adapt_to_n2(actions).map(N2Projection::into_parts)
    }

    pub fn adapt_to_n2(
        &self,
        actions: impl IntoIterator<Item = ActionId>,
    ) -> Result<N2Projection, N2AdapterError> {
        n2_adapter::to_n2_graph(self, actions)
    }

    pub fn all_to_n2_graph(&self) -> Result<(n2::graph::Graph, CommandArgMap), N2AdapterError> {
        self.to_n2_graph(self.action_ids())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionPlanMergeError {
    #[error("cannot compose execution plans: output `{path}` has incompatible producers")]
    ConflictingOutput { path: PathBuf },
}

#[derive(Default)]
pub(crate) struct ExecutionPlanBuilder {
    actions: Vec<ExecutionAction>,
    outputs: HashMap<PathBuf, DeclaredOutput>,
    artifacts: HashMap<ArtifactKey, Vec<PathBuf>>,
}

impl ExecutionPlanBuilder {
    pub(crate) fn add_action(
        &mut self,
        action: ExecutionAction,
        semantic_outputs: impl IntoIterator<Item = (ArtifactKey, Vec<PathBuf>)>,
    ) -> ActionId {
        let action_id = ActionId(self.actions.len());
        let mut artifacts_by_path = HashMap::new();

        for (artifact, paths) in semantic_outputs {
            assert!(
                !paths.is_empty(),
                "execution artifact has no physical outputs: {artifact:?}"
            );
            for path in &paths {
                let previous = artifacts_by_path.insert(path.clone(), artifact.clone());
                assert!(
                    previous.is_none(),
                    "declared execution output realizes multiple artifacts: {}",
                    path.display()
                );
            }
            let previous = self.artifacts.insert(artifact.clone(), paths);
            assert!(
                previous.is_none(),
                "execution artifact has multiple providers: {artifact:?}"
            );
        }

        for path in &action.outputs {
            self.add_output(action_id, path.clone(), artifacts_by_path.remove(path));
        }
        assert!(
            artifacts_by_path.is_empty(),
            "artifact output paths should be declared by their execution action"
        );

        self.actions.push(action);
        action_id
    }

    fn add_output(
        &mut self,
        producer: ActionId,
        path: PathBuf,
        artifact: Option<ArtifactKey>,
    ) -> PathBuf {
        let previous = self.outputs.insert(
            path.clone(),
            DeclaredOutput {
                producer,
                path: path.clone(),
                artifact,
            },
        );
        assert!(
            previous.is_none(),
            "declared execution output has multiple producers: {}",
            path.display()
        );
        path
    }

    pub(crate) fn finish(
        self,
        requested_artifacts: impl IntoIterator<Item = ArtifactKey>,
    ) -> ExecutionPlan {
        let mut seen = HashSet::new();
        let mut requested = Vec::new();
        for artifact in requested_artifacts {
            if !seen.insert(artifact.clone()) {
                continue;
            }
            let outputs = self
                .artifacts
                .get(&artifact)
                .unwrap_or_else(|| panic!("requested artifact has no realization: {artifact:?}"))
                .clone();
            requested.push((artifact, outputs));
        }
        ExecutionPlan {
            actions: self.actions,
            outputs: self.outputs,
            requested_artifacts: requested,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn execution_action(
        inputs: Vec<PathBuf>,
        semantic_outputs: Vec<(ArtifactKey, Vec<PathBuf>)>,
        declared_outputs: Vec<PathBuf>,
        command: &str,
    ) -> (ExecutionAction, Vec<(ArtifactKey, Vec<PathBuf>)>) {
        let outputs = semantic_outputs
            .iter()
            .flat_map(|(_, paths)| paths.iter().cloned())
            .chain(declared_outputs)
            .collect();
        (
            ExecutionAction::new(
                inputs,
                outputs,
                LoweredCommand::from(vec![command.to_string()]),
                command.to_string(),
                command.to_string(),
            ),
            semantic_outputs,
        )
    }

    fn producer_and_consumer_plan() -> (ExecutionPlan, ActionId, ActionId) {
        let mut builder = ExecutionPlanBuilder::default();
        let (action, outputs) = execution_action(
            Vec::new(),
            vec![(
                ArtifactKey::RuntimeLibrary,
                vec![PathBuf::from("build/libmoonbitrun.a")],
            )],
            Vec::new(),
            "archive-runtime",
        );
        let producer = builder.add_action(action, outputs);
        let (action, outputs) = execution_action(
            vec![PathBuf::from("build/libmoonbitrun.a")],
            Vec::new(),
            vec![PathBuf::from("build/app.dSYM")],
            "generate-debug-symbols",
        );
        let consumer = builder.add_action(action, outputs);
        let plan = builder.finish([ArtifactKey::RuntimeLibrary]);
        (plan, producer, consumer)
    }

    #[test]
    fn artifact_edges_and_physical_only_outputs_have_distinct_identity() {
        let (plan, producer, consumer) = producer_and_consumer_plan();

        let input = plan
            .declared_output(&plan.action(consumer).inputs()[0])
            .expect("action input should resolve to a declared output");
        assert_eq!(input.artifact(), Some(&ArtifactKey::RuntimeLibrary));
        assert_eq!(input.producer(), producer);
        assert_eq!(input.path(), Path::new("build/libmoonbitrun.a"));

        let debug_output = plan
            .declared_output(&plan.action(consumer).outputs()[0])
            .expect("action output should be declared");
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

    #[test]
    #[should_panic(expected = "execution artifact has no physical outputs")]
    fn semantic_artifact_requires_a_physical_output() {
        let (action, outputs) = execution_action(
            Vec::new(),
            vec![(ArtifactKey::RuntimeLibrary, Vec::new())],
            Vec::new(),
            "archive-runtime",
        );
        ExecutionPlanBuilder::default().add_action(action, outputs);
    }

    #[test]
    fn merge_reuses_an_identical_physical_provider_and_remaps_consumers() {
        let (first, producer, consumer) = producer_and_consumer_plan();
        let (second, _, _) = producer_and_consumer_plan();
        let mut composed = ExecutionPlan::default();

        assert_eq!(
            composed.merge(&first).expect("first plan should merge"),
            [producer, consumer]
        );
        assert_eq!(
            composed.merge(&second).expect("second plan should merge"),
            [producer, consumer]
        );
        assert_eq!(composed.action_ids().count(), 2);
        assert_eq!(
            composed
                .declared_output(Path::new("build/libmoonbitrun.a"))
                .expect("the shared output should be declared")
                .producer(),
            producer
        );
    }

    #[test]
    fn merge_rejects_different_actions_for_the_same_physical_output() {
        let (mut first, _, _) = producer_and_consumer_plan();
        let (mut second, _, _) = producer_and_consumer_plan();
        second.actions[0].description = "different archive command".to_string();

        first
            .merge(&second)
            .expect_err("incompatible producers should be rejected");
    }

    #[test]
    fn merge_remaps_plan_local_action_ids() {
        let (first, _, _) = producer_and_consumer_plan();
        let mut builder = ExecutionPlanBuilder::default();
        let (action, outputs) = execution_action(
            Vec::new(),
            Vec::new(),
            vec![PathBuf::from("build/other-output")],
            "other-command",
        );
        builder.add_action(action, outputs);
        let second = builder.finish([]);
        let mut composed = ExecutionPlan::default();

        composed.merge(&first).expect("first plan should merge");
        let remapped = composed.merge(&second).expect("second plan should merge");

        assert_eq!(remapped, [ActionId(2)]);
    }

    #[test]
    fn n2_projection_retains_build_to_action_provenance() {
        let (plan, producer, consumer) = producer_and_consumer_plan();
        let adapted = plan
            .adapt_to_n2([producer, consumer])
            .expect("execution plan should adapt to n2");

        for (index, expected) in [producer, consumer].into_iter().enumerate() {
            assert_eq!(
                adapted.action_for_build(n2::graph::BuildId::from(index)),
                Some(expected),
            );
        }
    }
}
