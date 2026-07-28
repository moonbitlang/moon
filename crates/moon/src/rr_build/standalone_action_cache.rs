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

//! Local action-to-output storage for single-file dependency preparation.
//!
//! Lowered actions are the only build-description input to this module. Valid
//! hits materialize their outputs directly; selected misses are converted to an
//! n2 graph through the Rupes Recta adapter.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use blake3::{Hash, Hasher};
use moonbuild_rupes_recta::{
    build_action_plan::BuildProduct,
    build_lower::{
        LoweredAction, LoweredCommand, LoweredCommandExecution, LoweredExternalInput,
        lowered_actions_to_n2_graph,
    },
    model::TargetKind,
};
use moonutil::user_log::UserLog;
use serde::{Deserialize, Serialize};

use super::{
    BuildConfig, BuildInput, CapturedBuildExecution, StandaloneDependencyInput,
    execute_build_capturing, finish_captured_build,
};

const FORMAT_VERSION: u32 = 1;
const OUTPUT_MANIFEST: &str = "manifest.json";
const PUBLISH_ATTEMPTS: usize = 8;

struct DependencyPreparation {
    input: Option<BuildInput>,
    actions_to_publish: Vec<DependencyAction>,
    external_inputs: Vec<ExternalInputSnapshot>,
    store: LocalActionOutputStore,
    _executor_state: Option<tempfile::TempDir>,
}

#[derive(Clone)]
struct DependencyAction {
    id: Hash,
    outputs: Vec<PathBuf>,
    cacheable: bool,
}

#[derive(Clone, Copy)]
struct ActionIdentity {
    digest: Hash,
    cacheable: bool,
}

struct ActionIdentities {
    actions: Vec<ActionIdentity>,
    external_inputs: Vec<ExternalInputSnapshot>,
}

#[derive(Clone)]
struct ExternalInputSnapshot {
    input: LoweredExternalInput,
    digest: Hash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestoreOutcome {
    Hit,
    Miss,
}

#[derive(Serialize, Deserialize)]
struct ActionEntry {
    version: u32,
    output_id: String,
}

#[derive(Serialize, Deserialize)]
struct OutputManifest {
    version: u32,
    files: Vec<OutputFile>,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
struct OutputFile {
    size: u64,
    digest: String,
}

#[derive(Clone)]
struct LocalActionOutputStore {
    root: PathBuf,
}

impl LocalActionOutputStore {
    fn new(target_dir: &Path) -> Self {
        Self {
            // Dependency artifacts already use `.mooncakes/<module>/...`.
            // Keep cache metadata in a reserved namespace so module names
            // cannot collide with action records or output objects.
            root: target_dir.join(".mooncakes").join(".build-cache"),
        }
    }

    fn work_root(&self) -> PathBuf {
        self.root.join("work")
    }

    fn action_entry_path(&self, action: &DependencyAction) -> PathBuf {
        self.root
            .join("actions")
            .join(format!("{}.json", action.id.to_hex()))
    }
}

pub(super) fn execute(
    cfg: &BuildConfig,
    input: StandaloneDependencyInput,
    target_dir: &Path,
    user_log: &UserLog,
) -> anyhow::Result<CapturedBuildExecution> {
    let DependencyPreparation {
        input,
        actions_to_publish,
        external_inputs,
        store,
        _executor_state,
    } = prepare(input, target_dir)?;
    let execution = match input {
        Some(input) => execute_build_capturing(cfg, input, target_dir)?,
        None => CapturedBuildExecution {
            n_tasks_executed: Some(0),
            diagnostics: Default::default(),
        },
    };
    if !execution.successful() {
        return Ok(execution);
    }

    if let Err(error) = verify_external_inputs(&external_inputs) {
        finish_captured_build(cfg, &execution, None, user_log);
        return Err(error);
    }
    for action in &actions_to_publish {
        if let Err(error) = store.publish(action) {
            finish_captured_build(cfg, &execution, None, user_log);
            return Err(error);
        }
    }
    Ok(execution)
}

fn prepare(
    input: StandaloneDependencyInput,
    target_dir: &Path,
) -> anyhow::Result<DependencyPreparation> {
    let store = LocalActionOutputStore::new(target_dir);
    let ActionIdentities {
        actions: identities,
        external_inputs,
    } = ActionIdentityBuilder::new(&input.actions).build()?;

    let mut missed_actions = Vec::new();
    let mut actions_to_publish = Vec::new();
    for (action, identity) in input.actions.into_iter().zip(identities) {
        let mut outputs = action
            .outputs()
            .iter()
            .flat_map(|product| product.paths().iter().cloned())
            .collect::<Vec<_>>();
        outputs.sort();
        outputs.dedup();
        let dependency_action = DependencyAction {
            id: identity.digest,
            cacheable: identity.cacheable && !outputs.is_empty(),
            outputs,
        };
        if store.restore(&dependency_action)? == RestoreOutcome::Miss {
            if dependency_action.cacheable {
                actions_to_publish.push(dependency_action);
            }
            missed_actions.push(action);
        }
    }

    if missed_actions.is_empty() {
        return Ok(DependencyPreparation {
            input: None,
            actions_to_publish,
            external_inputs,
            store,
            _executor_state: None,
        });
    }

    let work_root = store.work_root();
    std::fs::create_dir_all(&work_root).with_context(|| {
        format!(
            "Failed to create single-file dependency work directory at {}",
            work_root.display()
        )
    })?;
    let executor_state = tempfile::Builder::new()
        .prefix("n2-")
        .tempdir_in(&work_root)
        .context("Failed to create single-file dependency executor state")?;
    let db_path = executor_state.path().join("dependencies.moon_db");
    let (graph, command_args_by_output) = lowered_actions_to_n2_graph(missed_actions)?;

    Ok(DependencyPreparation {
        input: Some(BuildInput {
            graph,
            command_args_by_output,
            db_path,
        }),
        actions_to_publish,
        external_inputs,
        store,
        _executor_state: Some(executor_state),
    })
}

struct ActionIdentityBuilder<'a> {
    actions: &'a [LoweredAction],
    index_by_id: HashMap<moonbuild_rupes_recta::build_action_plan::BuildActionId, usize>,
    identities: Vec<Option<ActionIdentity>>,
    visiting: HashSet<usize>,
    external_digests: HashMap<LoweredExternalInput, Hash>,
}

impl<'a> ActionIdentityBuilder<'a> {
    fn new(actions: &'a [LoweredAction]) -> Self {
        let index_by_id = actions
            .iter()
            .enumerate()
            .map(|(index, action)| (action.id(), index))
            .collect();
        Self {
            actions,
            index_by_id,
            identities: vec![None; actions.len()],
            visiting: HashSet::new(),
            external_digests: HashMap::new(),
        }
    }

    fn build(mut self) -> anyhow::Result<ActionIdentities> {
        if self.index_by_id.len() != self.actions.len() {
            bail!("single-file dependency lowering contains duplicate action IDs");
        }
        for index in 0..self.actions.len() {
            self.identity_for(index)?;
        }
        let actions = self
            .identities
            .into_iter()
            .map(|identity| identity.expect("every dependency action should have an identity"))
            .collect();
        let mut external_inputs = self
            .external_digests
            .into_iter()
            .map(|(input, digest)| ExternalInputSnapshot { input, digest })
            .collect::<Vec<_>>();
        external_inputs.sort_by(|left, right| left.input.cmp(&right.input));
        Ok(ActionIdentities {
            actions,
            external_inputs,
        })
    }

    fn identity_for(&mut self, index: usize) -> anyhow::Result<ActionIdentity> {
        if let Some(identity) = self.identities[index] {
            return Ok(identity);
        }
        if !self.visiting.insert(index) {
            bail!("single-file dependency action graph contains a cycle");
        }

        let action = self.actions[index].clone();
        let mut fingerprint =
            FingerprintHasher::new(b"moon-single-file-dependency-lowered-action-v1");
        hash_command(&mut fingerprint, action.command());

        let mut external_inputs = action.external_inputs().to_vec();
        external_inputs.sort();
        external_inputs.dedup();
        fingerprint.sequence(b"external-inputs", external_inputs.len());
        for input in external_inputs {
            fingerprint.field(b"external-input-kind", external_input_kind(&input));
            fingerprint.field(
                b"external-input-path",
                input.path().as_os_str().as_encoded_bytes(),
            );
            let digest = self.external_digest(input)?;
            fingerprint.field(b"external-input-content", digest.as_bytes());
        }

        let mut cacheable = action.command().args().is_some()
            && !action.outputs().is_empty()
            && action
                .outputs()
                .iter()
                .all(|product| !product.paths().is_empty());
        let mut dependencies = Vec::with_capacity(action.dependencies().len());
        for dependency in action.dependencies() {
            let producer_index = self
                .index_by_id
                .get(&dependency.producer())
                .copied()
                .with_context(|| {
                    format!(
                        "single-file dependency action {:?} is missing producer {:?}",
                        action.id(),
                        dependency.producer()
                    )
                })?;
            let producer = self.identity_for(producer_index)?;
            cacheable &= producer.cacheable;

            let mut dependency_fingerprint =
                FingerprintHasher::new(b"moon-single-file-dependency-product-v1");
            dependency_fingerprint.field(b"producer-action", producer.digest.as_bytes());
            hash_logical_product(&mut dependency_fingerprint, dependency.product());
            let mut paths = dependency.paths().to_vec();
            paths.sort();
            paths.dedup();
            dependency_fingerprint.sequence(b"concrete-paths", paths.len());
            for path in paths {
                dependency_fingerprint.field(b"concrete-path", path.as_os_str().as_encoded_bytes());
            }
            dependencies.push(dependency_fingerprint.finish());
        }
        dependencies.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        fingerprint.sequence(b"dependencies", dependencies.len());
        for dependency in dependencies {
            fingerprint.field(b"dependency", dependency.as_bytes());
        }

        let mut output_paths = action
            .outputs()
            .iter()
            .flat_map(|product| product.paths().iter().cloned())
            .collect::<Vec<_>>();
        output_paths.sort();
        output_paths.dedup();
        fingerprint.sequence(b"output-paths", output_paths.len());
        for path in output_paths {
            fingerprint.field(b"output-path", path.as_os_str().as_encoded_bytes());
        }

        self.visiting.remove(&index);
        let identity = ActionIdentity {
            digest: fingerprint.finish(),
            cacheable,
        };
        self.identities[index] = Some(identity);
        Ok(identity)
    }

    fn external_digest(&mut self, input: LoweredExternalInput) -> anyhow::Result<Hash> {
        if let Some(digest) = self.external_digests.get(&input) {
            return Ok(*digest);
        }
        let digest = digest_external_input(&input).with_context(|| {
            format!(
                "Failed to identify single-file dependency input {}",
                input.path().display()
            )
        })?;
        self.external_digests.insert(input, digest);
        Ok(digest)
    }
}

fn hash_command(fingerprint: &mut FingerprintHasher, command: &LoweredCommand) {
    match command.args() {
        Some(args) => {
            fingerprint.field(b"command-kind", b"structured-args");
            fingerprint.sequence(b"arguments", args.len());
            for argument in args {
                fingerprint.field(b"argument", argument.as_bytes());
            }
        }
        None => {
            fingerprint.field(b"command-kind", b"verbatim");
        }
    }
    fingerprint.optional_path(b"working-directory", command.cwd());

    let environment = normalized_environment(command.env());
    fingerprint.sequence(b"environment", environment.len());
    for (name, value) in environment {
        fingerprint.field(b"environment-name", name.as_bytes());
        fingerprint.field(b"environment-value", value.as_bytes());
    }

    match command.execution() {
        LoweredCommandExecution::Inline(rendered) => {
            fingerprint.field(b"command-transport", b"inline");
            if command.args().is_none() {
                fingerprint.field(b"verbatim-command", rendered.as_bytes());
            }
        }
        LoweredCommandExecution::ResponseFile {
            command: rendered,
            file,
        } => {
            fingerprint.field(b"command-transport", b"response-file");
            if command.args().is_none() {
                fingerprint.field(b"verbatim-command", rendered.as_bytes());
            }
            fingerprint.field(
                b"response-file-path",
                file.path.as_os_str().as_encoded_bytes(),
            );
            fingerprint.field(b"response-file-content", file.content.as_bytes());
        }
    }
}

fn normalized_environment(environment: &[(String, String)]) -> Vec<(String, String)> {
    let mut normalized = BTreeMap::new();
    for (name, value) in environment {
        let identity_name = if cfg!(windows) {
            name.to_ascii_uppercase()
        } else {
            name.clone()
        };
        normalized.insert(identity_name, value.clone());
    }
    normalized.into_iter().collect()
}

fn hash_logical_product(fingerprint: &mut FingerprintHasher, product: &BuildProduct) {
    let (kind, target_kind, index, path) = match product {
        BuildProduct::PackageInterface { target } => (
            b"package-interface".as_slice(),
            Some(target.kind),
            None,
            None,
        ),
        BuildProduct::PackageCoreIr { target } => {
            (b"package-core-ir".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::ProofInterface { target } => {
            (b"proof-interface".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::ProofWhyml { target } => {
            (b"proof-whyml".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::ProofReport { target } => {
            (b"proof-report".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::CStubObject { index, .. } => {
            (b"c-stub-object".as_slice(), None, Some(*index), None)
        }
        BuildProduct::CStubLibrary { .. } => (b"c-stub-library".as_slice(), None, None, None),
        BuildProduct::LinkedCore { target } => {
            (b"linked-core".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::Executable { target } => {
            (b"executable".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::GeneratedTestDriver { target } => (
            b"generated-test-driver".as_slice(),
            Some(target.kind),
            None,
            None,
        ),
        BuildProduct::GeneratedTestMetadata { target } => (
            b"generated-test-metadata".as_slice(),
            Some(target.kind),
            None,
            None,
        ),
        BuildProduct::BundleResult { .. } => (b"bundle-result".as_slice(), None, None, None),
        BuildProduct::RuntimeLib => (b"runtime-lib".as_slice(), None, None, None),
        BuildProduct::GeneratedMbti { target } => {
            (b"generated-mbti".as_slice(), Some(target.kind), None, None)
        }
        BuildProduct::DocsDir => (b"docs-dir".as_slice(), None, None, None),
        BuildProduct::VirtualPackageInterface { .. } => {
            (b"virtual-package-interface".as_slice(), None, None, None)
        }
        BuildProduct::MoonLexGeneratedSource { index, .. } => (
            b"moonlex-generated-source".as_slice(),
            None,
            Some(*index),
            None,
        ),
        BuildProduct::MoonYaccGeneratedSource { index, .. } => (
            b"moonyacc-generated-source".as_slice(),
            None,
            Some(*index),
            None,
        ),
        BuildProduct::PrebuildOutputPath { path } => (
            b"prebuild-output-path".as_slice(),
            None,
            None,
            Some(path.as_path()),
        ),
    };
    fingerprint.field(b"logical-product", kind);
    if let Some(target_kind) = target_kind {
        fingerprint.field(b"target-kind", target_kind_name(target_kind));
    }
    if let Some(index) = index {
        fingerprint.field(b"product-index", &index.to_le_bytes());
    }
    if let Some(path) = path {
        fingerprint.field(b"logical-product-path", path.as_os_str().as_encoded_bytes());
    }
}

fn target_kind_name(kind: TargetKind) -> &'static [u8] {
    match kind {
        TargetKind::Source => b"source",
        TargetKind::WhiteboxTest => b"whitebox-test",
        TargetKind::BlackboxTest => b"blackbox-test",
        TargetKind::InlineTest => b"inline-test",
        TargetKind::SubPackage => b"subpackage",
    }
}

fn external_input_kind(input: &LoweredExternalInput) -> &'static [u8] {
    match input {
        LoweredExternalInput::File(_) => b"file",
        LoweredExternalInput::StandardLibraryInterfaces(_) => b"stdlib-interfaces",
    }
}

struct FingerprintHasher {
    hasher: Hasher,
}

impl FingerprintHasher {
    fn new(domain: &[u8]) -> Self {
        let mut this = Self {
            hasher: Hasher::new(),
        };
        this.field(b"domain", domain);
        this
    }

    fn field(&mut self, name: &[u8], value: &[u8]) {
        self.bytes(name);
        self.bytes(value);
    }

    fn optional_path(&mut self, name: &[u8], value: Option<&Path>) {
        self.field(name, &[value.is_some() as u8]);
        if let Some(value) = value {
            self.field(b"value", value.as_os_str().as_encoded_bytes());
        }
    }

    fn sequence(&mut self, name: &[u8], len: usize) {
        self.field(name, &(len as u64).to_le_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.hasher.update(&(value.len() as u64).to_le_bytes());
        self.hasher.update(value);
    }

    fn finish(self) -> Hash {
        self.hasher.finalize()
    }
}

fn digest_external_input(input: &LoweredExternalInput) -> anyhow::Result<Hash> {
    let path = input.path();
    let metadata = std::fs::metadata(path)?;
    match input {
        LoweredExternalInput::File(_) if metadata.is_file() => digest_file(path),
        LoweredExternalInput::File(_) => bail!(
            "single-file dependency input is not a regular file: {}",
            path.display()
        ),
        LoweredExternalInput::StandardLibraryInterfaces(_) if metadata.is_dir() => {
            digest_stdlib_interfaces(path)
        }
        LoweredExternalInput::StandardLibraryInterfaces(_) => bail!(
            "single-file standard-library input is not a directory: {}",
            path.display()
        ),
    }
}

fn digest_stdlib_interfaces(root: &Path) -> anyhow::Result<Hash> {
    let mut fingerprint = FingerprintHasher::new(b"moon-single-file-stdlib-interfaces-v1");
    let mut pending = vec![PathBuf::new()];
    let mut interfaces = Vec::new();
    while let Some(relative_dir) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&relative_dir))? {
            let entry = entry?;
            let relative_path = relative_dir.join(entry.file_name());
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(relative_path);
            } else if file_type.is_file()
                && relative_path
                    .extension()
                    .is_some_and(|extension| extension == "mi")
            {
                interfaces.push(relative_path);
            } else if !file_type.is_file() {
                bail!(
                    "single-file standard-library directory contains an unsupported entry: {}",
                    entry.path().display()
                );
            }
        }
    }
    interfaces.sort();
    for relative_path in interfaces {
        fingerprint.field(b"file", relative_path.as_os_str().as_encoded_bytes());
        fingerprint.field(
            b"content",
            digest_file(&root.join(relative_path))?.as_bytes(),
        );
    }
    Ok(fingerprint.finish())
}

fn digest_file(path: &Path) -> anyhow::Result<Hash> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

fn verify_external_inputs(inputs: &[ExternalInputSnapshot]) -> anyhow::Result<()> {
    for input in inputs {
        let current = digest_external_input(&input.input).with_context(|| {
            format!(
                "Failed to revalidate single-file dependency input {}",
                input.input.path().display()
            )
        })?;
        if current != input.digest {
            bail!(
                "single-file dependency input changed during preparation: {}",
                input.input.path().display()
            );
        }
    }
    Ok(())
}

impl LocalActionOutputStore {
    fn restore(&self, action: &DependencyAction) -> anyhow::Result<RestoreOutcome> {
        if !action.cacheable {
            return Ok(RestoreOutcome::Miss);
        }
        let entry_path = self.action_entry_path(action);
        let contents = match std::fs::read(&entry_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RestoreOutcome::Miss);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read {}", entry_path.display()));
            }
        };
        let Ok(entry) = serde_json::from_slice::<ActionEntry>(&contents) else {
            return Ok(RestoreOutcome::Miss);
        };
        if entry.version != FORMAT_VERSION || !is_blake3_hex(&entry.output_id) {
            return Ok(RestoreOutcome::Miss);
        }

        let object_dir = self.root.join("outputs").join(&entry.output_id);
        let Some(manifest) = read_output_manifest(&object_dir, &entry.output_id)? else {
            return Ok(RestoreOutcome::Miss);
        };
        if manifest.files.len() != action.outputs.len()
            || !object_files_match(&object_dir, &manifest.files)?
        {
            return Ok(RestoreOutcome::Miss);
        }

        let mut staged = Vec::new();
        for (index, (destination, expected)) in
            action.outputs.iter().zip(&manifest.files).enumerate()
        {
            if file_matches(destination, expected)? {
                continue;
            }
            let parent = destination.parent().with_context(|| {
                format!(
                    "Single-file dependency output has no parent: {}",
                    destination.display()
                )
            })?;
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create single-file dependency output directory {}",
                    parent.display()
                )
            })?;
            let staged_file = tempfile::NamedTempFile::new_in(parent).with_context(|| {
                format!(
                    "Failed to stage single-file dependency output {}",
                    destination.display()
                )
            })?;
            std::fs::copy(object_dir.join(index.to_string()), staged_file.path()).with_context(
                || {
                    format!(
                        "Failed to copy cached single-file dependency output {}",
                        destination.display()
                    )
                },
            )?;
            staged.push((staged_file, destination, expected));
        }

        for (staged_file, destination, expected) in staged {
            match staged_file.persist(destination) {
                Ok(_) => {}
                Err(_error) if file_matches(destination, expected)? => {}
                Err(error) => {
                    return Err(error.error).with_context(|| {
                        format!(
                            "Failed to materialize single-file dependency output {}",
                            destination.display()
                        )
                    });
                }
            }
        }
        Ok(RestoreOutcome::Hit)
    }

    fn publish(&self, action: &DependencyAction) -> anyhow::Result<()> {
        if !action.cacheable {
            return Ok(());
        }
        let files = action
            .outputs
            .iter()
            .map(|path| {
                let metadata = std::fs::metadata(path).with_context(|| {
                    format!(
                        "Single-file dependency action did not produce {}",
                        path.display()
                    )
                })?;
                if !metadata.is_file() {
                    return Ok(None);
                }
                Ok(Some(OutputFile {
                    size: metadata.len(),
                    digest: digest_file(path)?.to_hex().to_string(),
                }))
            })
            .collect::<anyhow::Result<Option<Vec<_>>>>()?;
        let Some(files) = files else {
            return Ok(());
        };
        let manifest = OutputManifest {
            version: FORMAT_VERSION,
            files,
        };
        let manifest_contents =
            serde_json::to_vec(&manifest).context("Failed to serialize output manifest")?;
        let output_id = blake3::hash(&manifest_contents).to_hex().to_string();

        self.publish_output_object(action, &output_id, &manifest, &manifest_contents)?;
        self.publish_action_entry(action, &output_id)
    }

    fn publish_output_object(
        &self,
        action: &DependencyAction,
        output_id: &str,
        manifest: &OutputManifest,
        manifest_contents: &[u8],
    ) -> anyhow::Result<()> {
        let outputs_root = self.root.join("outputs");
        std::fs::create_dir_all(&outputs_root).with_context(|| {
            format!(
                "Failed to create output object directory at {}",
                outputs_root.display()
            )
        })?;
        let object_dir = outputs_root.join(output_id);
        if output_object_matches(&object_dir, output_id, &manifest.files)? {
            return Ok(());
        }

        let staging = tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&outputs_root)
            .context("Failed to stage output object")?;
        for (index, output) in action.outputs.iter().enumerate() {
            std::fs::copy(output, staging.path().join(index.to_string())).with_context(|| {
                format!(
                    "Failed to stage single-file dependency output {}",
                    output.display()
                )
            })?;
        }
        std::fs::write(staging.path().join(OUTPUT_MANIFEST), manifest_contents)
            .context("Failed to stage output manifest")?;

        let mut last_error = None;
        for _ in 0..PUBLISH_ATTEMPTS {
            match std::fs::rename(staging.path(), &object_dir) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if output_object_matches(&object_dir, output_id, &manifest.files)? {
                        return Ok(());
                    }
                    match quarantine_path(&object_dir, &outputs_root) {
                        Ok(()) => {}
                        Err(error) => last_error = Some(error),
                    }
                }
            }
        }
        Err(last_error.expect("publishing attempted at least once"))
            .with_context(|| format!("Failed to publish output object {}", object_dir.display()))
    }

    fn publish_action_entry(
        &self,
        action: &DependencyAction,
        output_id: &str,
    ) -> anyhow::Result<()> {
        let actions_root = self.root.join("actions");
        std::fs::create_dir_all(&actions_root).with_context(|| {
            format!(
                "Failed to create action entry directory at {}",
                actions_root.display()
            )
        })?;
        let entry_path = self.action_entry_path(action);
        let entry = ActionEntry {
            version: FORMAT_VERSION,
            output_id: output_id.to_owned(),
        };

        let mut last_error = None;
        for _ in 0..PUBLISH_ATTEMPTS {
            let mut staged_entry = tempfile::NamedTempFile::new_in(&actions_root)
                .context("Failed to stage action entry")?;
            serde_json::to_writer(&mut staged_entry, &entry)
                .context("Failed to serialize action entry")?;
            staged_entry
                .flush()
                .context("Failed to flush action entry")?;
            match staged_entry.persist(&entry_path) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    last_error = Some(error.error);
                    if self.action_entry_is_usable(&entry_path, action.outputs.len())? {
                        return Ok(());
                    }
                    match quarantine_path(&entry_path, &actions_root) {
                        Ok(()) => {}
                        Err(error) => last_error = Some(error),
                    }
                }
            }
        }
        Err(last_error.expect("publishing attempted at least once"))
            .with_context(|| format!("Failed to publish action entry {}", entry_path.display()))
    }

    fn action_entry_is_usable(
        &self,
        entry_path: &Path,
        output_count: usize,
    ) -> anyhow::Result<bool> {
        let contents = match std::fs::read(entry_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let Ok(entry) = serde_json::from_slice::<ActionEntry>(&contents) else {
            return Ok(false);
        };
        if entry.version != FORMAT_VERSION || !is_blake3_hex(&entry.output_id) {
            return Ok(false);
        }
        let object_dir = self.root.join("outputs").join(&entry.output_id);
        let Some(manifest) = read_output_manifest(&object_dir, &entry.output_id)? else {
            return Ok(false);
        };
        Ok(manifest.files.len() == output_count
            && object_files_match(&object_dir, &manifest.files)?)
    }
}

fn quarantine_path(path: &Path, parent: &Path) -> std::io::Result<()> {
    let quarantine = tempfile::Builder::new()
        .prefix(".replaced-")
        .tempdir_in(parent)?;
    match std::fs::rename(path, quarantine.path().join("entry")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn read_output_manifest(
    object_dir: &Path,
    output_id: &str,
) -> anyhow::Result<Option<OutputManifest>> {
    let manifest_path = object_dir.join(OUTPUT_MANIFEST);
    let contents = match std::fs::read(&manifest_path) {
        Ok(contents) => contents,
        Err(error) if is_missing_or_wrong_type(error.kind()) => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Failed to read output manifest {}", manifest_path.display())
            });
        }
    };
    if blake3::hash(&contents).to_hex().as_str() != output_id {
        return Ok(None);
    }
    let Ok(manifest) = serde_json::from_slice::<OutputManifest>(&contents) else {
        return Ok(None);
    };
    Ok((manifest.version == FORMAT_VERSION
        && manifest
            .files
            .iter()
            .all(|file| is_blake3_hex(&file.digest)))
    .then_some(manifest))
}

fn output_object_matches(
    object_dir: &Path,
    output_id: &str,
    expected: &[OutputFile],
) -> anyhow::Result<bool> {
    let Some(manifest) = read_output_manifest(object_dir, output_id)? else {
        return Ok(false);
    };
    Ok(manifest.files == expected && object_files_match(object_dir, &manifest.files)?)
}

fn object_files_match(object_dir: &Path, files: &[OutputFile]) -> anyhow::Result<bool> {
    for (index, expected) in files.iter().enumerate() {
        if !file_matches(&object_dir.join(index.to_string()), expected)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn is_blake3_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn file_matches(path: &Path, expected: &OutputFile) -> anyhow::Result<bool> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if is_missing_or_wrong_type(error.kind()) => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to inspect cached output {}", path.display()));
        }
    };
    if !metadata.is_file() || metadata.len() != expected.size {
        return Ok(false);
    }
    match digest_file(path) {
        Ok(digest) => Ok(digest.to_hex().as_str() == expected.digest),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| is_missing_or_wrong_type(error.kind())) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn is_missing_or_wrong_type(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::NotFound
            | ErrorKind::NotADirectory
            | ErrorKind::IsADirectory
            | ErrorKind::InvalidInput
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    fn cached_action(
        root: &Path,
        outputs: &[(&str, &str)],
    ) -> (LocalActionOutputStore, DependencyAction) {
        let store = LocalActionOutputStore::new(root);
        let output_paths = outputs
            .iter()
            .map(|(name, content)| {
                let path = root.join("build").join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, content).unwrap();
                path
            })
            .collect();
        (
            store,
            DependencyAction {
                id: blake3::hash(b"action"),
                outputs: output_paths,
                cacheable: true,
            },
        )
    }

    fn action_entry(store: &LocalActionOutputStore, action: &DependencyAction) -> ActionEntry {
        serde_json::from_slice(&std::fs::read(store.action_entry_path(action)).unwrap()).unwrap()
    }

    #[test]
    fn environment_identity_is_order_independent_and_last_value_wins() {
        assert_eq!(
            normalized_environment(&[
                ("B".into(), "2".into()),
                ("A".into(), "old".into()),
                ("A".into(), "1".into()),
            ]),
            normalized_environment(&[
                ("A".into(), "old".into()),
                ("B".into(), "2".into()),
                ("A".into(), "1".into()),
            ])
        );
    }

    #[test]
    fn structured_arguments_have_stable_unambiguous_identity() {
        let digest = |arguments: &[&str]| {
            let command = LoweredCommand::from(
                arguments
                    .iter()
                    .map(|argument| (*argument).to_owned())
                    .collect::<Vec<_>>(),
            );
            let mut fingerprint = FingerprintHasher::new(b"test-command");
            hash_command(&mut fingerprint, &command);
            fingerprint.finish()
        };

        assert_eq!(digest(&["tool", "a b"]), digest(&["tool", "a b"]));
        assert_ne!(digest(&["tool", "ab", "c"]), digest(&["tool", "a", "bc"]));
    }

    #[test]
    fn stdlib_identity_uses_only_sorted_interface_paths_and_contents() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("nested")).unwrap();
        std::fs::write(root.path().join("nested/b.mi"), "b").unwrap();
        std::fs::write(root.path().join("a.mi"), "a").unwrap();
        std::fs::write(root.path().join("ignored.core"), "one").unwrap();
        let first = digest_stdlib_interfaces(root.path()).unwrap();

        std::fs::write(root.path().join("ignored.core"), "two").unwrap();
        assert_eq!(digest_stdlib_interfaces(root.path()).unwrap(), first);
        std::fs::write(root.path().join("nested/b.mi"), "changed").unwrap();
        assert_ne!(digest_stdlib_interfaces(root.path()).unwrap(), first);
    }

    #[test]
    fn corrupted_manifest_is_a_miss() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        let entry = action_entry(&store, &action);
        std::fs::write(
            store
                .root
                .join("outputs")
                .join(entry.output_id)
                .join(OUTPUT_MANIFEST),
            b"corrupt",
        )
        .unwrap();

        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
    }

    #[test]
    fn damaged_missing_and_partial_objects_are_safe_misses() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(
            root.path(),
            &[("dependency.mi", "interface"), ("dependency.core", "core")],
        );
        store.publish(&action).unwrap();
        let entry = action_entry(&store, &action);
        let object_dir = store.root.join("outputs").join(entry.output_id);

        std::fs::write(object_dir.join("1"), "bad!").unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
        store
            .publish(&action)
            .expect("publishing should repair a damaged object");
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);

        std::fs::remove_file(object_dir.join("1")).unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);

        std::fs::remove_dir_all(object_dir).unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
    }

    #[test]
    fn malformed_and_old_action_entries_are_misses() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        let entry_path = store.action_entry_path(&action);
        std::fs::create_dir_all(entry_path.parent().unwrap()).unwrap();
        std::fs::write(&entry_path, b"not-json").unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);

        std::fs::write(
            &entry_path,
            serde_json::to_vec(&ActionEntry {
                version: FORMAT_VERSION - 1,
                output_id: blake3::hash(b"old").to_hex().to_string(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Miss);
    }

    #[test]
    fn duplicate_publish_succeeds_and_restores_complete_outputs() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(
            root.path(),
            &[("dependency.mi", "interface"), ("dependency.core", "core")],
        );
        store.publish(&action).unwrap();
        store.publish(&action).unwrap();
        for output in &action.outputs {
            std::fs::remove_file(output).unwrap();
        }

        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);
        assert_eq!(
            std::fs::read_to_string(&action.outputs[0]).unwrap(),
            "interface"
        );
        assert_eq!(std::fs::read_to_string(&action.outputs[1]).unwrap(), "core");
    }

    #[test]
    fn concurrent_publish_loser_validates_winner() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(8));
        let writers = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let action = Arc::clone(&action);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.publish(&action)
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap().unwrap();
        }
        std::fs::remove_file(&action.outputs[0]).unwrap();
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);
    }

    #[test]
    fn concurrent_publish_repairs_corrupted_object_without_failure() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        let entry = action_entry(&store, &action);
        std::fs::write(
            store
                .root
                .join("outputs")
                .join(entry.output_id)
                .join(OUTPUT_MANIFEST),
            b"corrupt",
        )
        .unwrap();

        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(8));
        let writers = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let action = Arc::clone(&action);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.publish(&action)
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap().unwrap();
        }
        assert_eq!(store.restore(&action).unwrap(), RestoreOutcome::Hit);
    }

    #[test]
    fn concurrent_restore_and_publish_remain_successful() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(3));

        let publisher = {
            let store = Arc::clone(&store);
            let action = Arc::clone(&action);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    store.publish(&action)?;
                }
                anyhow::Ok(())
            })
        };
        let restorer = {
            let store = Arc::clone(&store);
            let action = Arc::clone(&action);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    assert_eq!(store.restore(&action)?, RestoreOutcome::Hit);
                }
                anyhow::Ok(())
            })
        };
        barrier.wait();
        publisher.join().unwrap().unwrap();
        restorer.join().unwrap().unwrap();
    }

    #[test]
    fn concurrent_restores_materialize_the_same_output() {
        let root = tempfile::tempdir().unwrap();
        let (store, action) = cached_action(root.path(), &[("dependency.mi", "interface")]);
        store.publish(&action).unwrap();
        std::fs::remove_file(&action.outputs[0]).unwrap();

        let store = Arc::new(store);
        let action = Arc::new(action);
        let barrier = Arc::new(Barrier::new(8));
        let restorers = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let action = Arc::clone(&action);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.restore(&action)
                })
            })
            .collect::<Vec<_>>();
        for restorer in restorers {
            assert_eq!(restorer.join().unwrap().unwrap(), RestoreOutcome::Hit);
        }
        assert_eq!(
            std::fs::read_to_string(&action.outputs[0]).unwrap(),
            "interface"
        );
    }
}
