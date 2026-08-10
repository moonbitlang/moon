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

//! Canonical identities for concrete Rupes Recta actions.
//!
//! The identity boundary consumes [`LoweredAction`] directly. It does not
//! inspect n2's rendered graph, and the opaque `BuildActionId` values used to
//! connect actions within one lowering are discarded before hashing.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs::File,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use blake3::Hasher;
use moonbuild_rupes_recta::{
    build_action_plan::BuildProduct,
    build_lower::{LoweredAction, LoweredCommandExecution, LoweredExternalInput},
    build_plan::ArtifactKey,
    model::TargetKind,
};

/// Invocation state observed by every executed action.
///
/// The working directory and environment are passed explicitly so the caller
/// hashes the same inherited process state that it gives to n2.
#[derive(Clone, Debug)]
pub struct ActionIdentityContext {
    inherited_working_directory: PathBuf,
    inherited_environment: Vec<(OsString, OsString)>,
}

impl ActionIdentityContext {
    pub fn new(
        inherited_working_directory: impl Into<PathBuf>,
        inherited_environment: Vec<(OsString, OsString)>,
    ) -> Self {
        Self {
            inherited_working_directory: inherited_working_directory.into(),
            inherited_environment,
        }
    }
}

/// A stable BLAKE3 digest for one canonical action.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ActionDigest([u8; 32]);

impl ActionDigest {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

impl std::fmt::Debug for ActionDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// The identity and reuse eligibility of one lowered action.
///
/// Cache-ineligible actions still receive a structural identity, but their
/// unmodeled external input contents are deliberately not read. Their
/// ineligibility propagates to every consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionIdentity {
    digest: ActionDigest,
    cacheable: bool,
}

impl ActionIdentity {
    pub fn digest(self) -> ActionDigest {
        self.digest
    }

    pub fn is_cacheable(self) -> bool {
        self.cacheable
    }
}

/// Compute identities in the same order as `actions`.
///
/// Every producer referenced by these actions must occur in the same slice.
/// This makes the recursive dependency closure explicit while keeping
/// `BuildActionId` local to this lowering.
pub fn compute_action_identities(
    actions: &[LoweredAction],
    context: &ActionIdentityContext,
) -> anyhow::Result<Vec<ActionIdentity>> {
    let index_by_id = actions
        .iter()
        .enumerate()
        .map(|(index, action)| (action.id(), index))
        .collect::<HashMap<_, _>>();
    if index_by_id.len() != actions.len() {
        bail!("lowered action set contains duplicate action IDs");
    }

    let actions = actions
        .iter()
        .map(|action| {
            let dependencies = action
                .dependencies()
                .iter()
                .map(|product| {
                    let producer =
                        index_by_id
                            .get(&product.producer())
                            .copied()
                            .with_context(|| {
                                format!(
                                    "lowered action {:?} is missing producer {:?}",
                                    action.id(),
                                    product.producer()
                                )
                            })?;
                    Ok(CanonicalProduct {
                        producer: Some(producer),
                        logical: LogicalProduct::from(product.product()),
                        paths: product.paths().to_vec(),
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let external_inputs = action.external_inputs().to_vec();
            let outputs = action
                .outputs()
                .iter()
                .map(|product| CanonicalProduct {
                    producer: None,
                    logical: LogicalProduct::from(product.product()),
                    paths: product.paths().to_vec(),
                })
                .collect();
            let execution = match action.command().execution() {
                LoweredCommandExecution::Inline(command) => CanonicalExecution::Inline {
                    command: command.clone(),
                },
                LoweredCommandExecution::ResponseFile { command, file } => {
                    CanonicalExecution::ResponseFile {
                        command: command.clone(),
                        path: file.path.clone(),
                        content: file.content.clone(),
                    }
                }
            };
            Ok(CanonicalAction {
                dependencies,
                external_inputs,
                outputs,
                command: CanonicalCommand {
                    args: action.command().args().to_vec(),
                    execution,
                    cwd: action.command().cwd().map(ToOwned::to_owned),
                    environment: action.command().env().to_vec(),
                },
                cache_eligible: action.is_cache_eligible(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    compute_canonical_actions(&actions, context)
}

#[derive(Clone)]
struct CanonicalAction {
    dependencies: Vec<CanonicalProduct>,
    external_inputs: Vec<LoweredExternalInput>,
    outputs: Vec<CanonicalProduct>,
    command: CanonicalCommand,
    cache_eligible: bool,
}

#[derive(Clone)]
struct CanonicalProduct {
    producer: Option<usize>,
    logical: LogicalProduct,
    paths: Vec<PathBuf>,
}

#[derive(Clone)]
struct LogicalProduct {
    kind: &'static [u8],
    target_kind: Option<TargetKind>,
    index: Option<u32>,
    path: Option<PathBuf>,
}

impl From<&BuildProduct> for LogicalProduct {
    fn from(product: &BuildProduct) -> Self {
        let (kind, target_kind, index, path) = match product {
            BuildProduct::Artifact(ArtifactKey::CheckMi { target_kind, .. }) => {
                (b"check-mi".as_slice(), Some(*target_kind), None, None)
            }
            BuildProduct::Artifact(ArtifactKey::BuildMi { target_kind, .. }) => {
                (b"build-mi".as_slice(), Some(*target_kind), None, None)
            }
            BuildProduct::Artifact(ArtifactKey::CoreIr { target_kind, .. }) => {
                (b"core-ir".as_slice(), Some(*target_kind), None, None)
            }
            BuildProduct::Artifact(ArtifactKey::VirtualContractMi { .. }) => {
                (b"virtual-contract-mi".as_slice(), None, None, None)
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
            BuildProduct::DsymBundle { target } => {
                (b"dsym-bundle".as_slice(), Some(target.kind), None, None)
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
            BuildProduct::RuntimeObject { index } => {
                (b"runtime-object".as_slice(), None, Some(*index), None)
            }
            BuildProduct::RuntimeLib => (b"runtime-lib".as_slice(), None, None, None),
            BuildProduct::GeneratedMbti { target } => {
                (b"generated-mbti".as_slice(), Some(target.kind), None, None)
            }
            BuildProduct::DocsDir => (b"docs-dir".as_slice(), None, None, None),
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
                Some(path.clone()),
            ),
        };
        Self {
            kind,
            target_kind,
            index,
            path,
        }
    }
}

#[derive(Clone)]
struct CanonicalCommand {
    args: Vec<String>,
    execution: CanonicalExecution,
    cwd: Option<PathBuf>,
    environment: Vec<(String, String)>,
}

#[derive(Clone)]
enum CanonicalExecution {
    Inline {
        command: String,
    },
    ResponseFile {
        command: String,
        path: PathBuf,
        content: String,
    },
}

fn compute_canonical_actions(
    actions: &[CanonicalAction],
    context: &ActionIdentityContext,
) -> anyhow::Result<Vec<ActionIdentity>> {
    if !context.inherited_working_directory.is_absolute() {
        bail!(
            "inherited working directory must be absolute: {}",
            context.inherited_working_directory.display()
        );
    }
    ActionIdentityBuilder {
        actions,
        context,
        identities: vec![None; actions.len()],
        visiting: HashSet::new(),
        external_digests: HashMap::new(),
        file_digests: HashMap::new(),
    }
    .build()
}

struct ActionIdentityBuilder<'a> {
    actions: &'a [CanonicalAction],
    context: &'a ActionIdentityContext,
    identities: Vec<Option<ActionIdentity>>,
    visiting: HashSet<usize>,
    external_digests: HashMap<LoweredExternalInput, ActionDigest>,
    file_digests: HashMap<PathBuf, ActionDigest>,
}

impl ActionIdentityBuilder<'_> {
    fn build(mut self) -> anyhow::Result<Vec<ActionIdentity>> {
        for index in 0..self.actions.len() {
            self.identity_for(index)?;
        }
        Ok(self
            .identities
            .into_iter()
            .map(|identity| identity.expect("every action should have an identity"))
            .collect())
    }

    fn identity_for(&mut self, index: usize) -> anyhow::Result<ActionIdentity> {
        if let Some(identity) = self.identities.get(index).copied().flatten() {
            return Ok(identity);
        }
        if index >= self.actions.len() {
            bail!("lowered action references missing producer index {index}");
        }
        if !self.visiting.insert(index) {
            bail!("lowered action graph contains a cycle");
        }

        let mut fingerprint = FingerprintHasher::new(b"moon-lowered-action-v1");
        let (external_inputs, dependencies, outputs, mut cacheable, hash_external_contents) = {
            let action = &self.actions[index];
            self.hash_command(&mut fingerprint, &action.command);

            let mut outputs = action
                .outputs
                .iter()
                .map(|output| {
                    let mut output_fingerprint =
                        FingerprintHasher::new(b"moon-lowered-product-output-v1");
                    self.hash_product(&mut output_fingerprint, output);
                    output_fingerprint.finish()
                })
                .collect::<Vec<_>>();
            outputs.sort_unstable_by_key(|digest| digest.0);
            outputs.dedup();

            (
                action.external_inputs.clone(),
                action.dependencies.clone(),
                outputs,
                action.cache_eligible
                    && !action.command.args.is_empty()
                    && !action.outputs.is_empty()
                    && action
                        .outputs
                        .iter()
                        .all(|product| !product.paths.is_empty()),
                action.cache_eligible,
            )
        };

        let mut external_input_digests = Vec::with_capacity(external_inputs.len());
        for input in external_inputs {
            let mut input_fingerprint = FingerprintHasher::new(b"moon-lowered-external-input-v1");
            input_fingerprint.field(
                b"kind",
                match input {
                    LoweredExternalInput::File(_) => b"file",
                    LoweredExternalInput::StandardLibraryInterfaces(_) => b"stdlib-interfaces",
                },
            );
            input_fingerprint.field(b"path", path_bytes(input.path()));
            // Ineligible actions can contain directory-shaped or otherwise
            // unmodeled observations. Their digest is diagnostic structure,
            // never a reusable cache key.
            if hash_external_contents {
                input_fingerprint.field(b"content", self.external_digest(&input)?.as_bytes());
            }
            external_input_digests.push(input_fingerprint.finish());
        }
        external_input_digests.sort_unstable_by_key(|digest| digest.0);
        external_input_digests.dedup();
        fingerprint.sequence(b"external-inputs", external_input_digests.len());
        for input in external_input_digests {
            fingerprint.field(b"external-input", input.as_bytes());
        }

        let mut dependency_digests = Vec::with_capacity(dependencies.len());
        for dependency in dependencies {
            let producer = dependency
                .producer
                .context("lowered dependency has no producer")?;
            let producer = self.identity_for(producer)?;
            cacheable &= producer.cacheable;

            let mut dependency_fingerprint =
                FingerprintHasher::new(b"moon-lowered-product-dependency-v1");
            dependency_fingerprint.field(b"producer-action", producer.digest.as_bytes());
            self.hash_product(&mut dependency_fingerprint, &dependency);
            dependency_digests.push(dependency_fingerprint.finish());
        }
        dependency_digests.sort_unstable_by_key(|digest| digest.0);
        dependency_digests.dedup();
        fingerprint.sequence(b"dependencies", dependency_digests.len());
        for dependency in dependency_digests {
            fingerprint.field(b"dependency", dependency.as_bytes());
        }

        fingerprint.sequence(b"outputs", outputs.len());
        for output in outputs {
            fingerprint.field(b"output", output.as_bytes());
        }

        self.visiting.remove(&index);
        let identity = ActionIdentity {
            digest: fingerprint.finish(),
            cacheable,
        };
        self.identities[index] = Some(identity);
        Ok(identity)
    }

    fn hash_command(&self, fingerprint: &mut FingerprintHasher, command: &CanonicalCommand) {
        fingerprint.sequence(b"arguments", command.args.len());
        for argument in &command.args {
            fingerprint.field(b"argument", argument.as_bytes());
        }

        let effective_working_directory = match &command.cwd {
            Some(cwd) if cwd.is_absolute() => Cow::Borrowed(cwd.as_path()),
            Some(cwd) => Cow::Owned(self.context.inherited_working_directory.join(cwd)),
            None => Cow::Borrowed(self.context.inherited_working_directory.as_path()),
        };
        fingerprint.field(
            b"working-directory",
            path_bytes(&effective_working_directory),
        );

        let environment =
            normalized_environment(&self.context.inherited_environment, &command.environment);
        fingerprint.sequence(b"environment", environment.len());
        for (name, value) in environment {
            fingerprint.field(b"environment-name", &name);
            fingerprint.field(b"environment-value", &value);
        }

        match &command.execution {
            CanonicalExecution::Inline { command } => {
                fingerprint.field(b"command-transport", b"inline");
                fingerprint.field(b"transport-command", command.as_bytes());
            }
            CanonicalExecution::ResponseFile {
                command,
                path,
                content,
            } => {
                fingerprint.field(b"command-transport", b"response-file");
                fingerprint.field(b"transport-command", command.as_bytes());
                fingerprint.field(b"response-file-path", path_bytes(path));
                fingerprint.field(b"response-file-content", content.as_bytes());
            }
        }
    }

    fn hash_product(&self, fingerprint: &mut FingerprintHasher, product: &CanonicalProduct) {
        fingerprint.field(b"logical-product", product.logical.kind);
        if let Some(kind) = product.logical.target_kind {
            fingerprint.field(b"target-kind", target_kind_name(kind));
        }
        if let Some(index) = product.logical.index {
            fingerprint.field(b"product-index", &index.to_le_bytes());
        }
        if let Some(path) = &product.logical.path {
            fingerprint.field(b"logical-product-path", path_bytes(path));
        }

        let mut paths = product
            .paths
            .iter()
            .map(|path| path_bytes(path).to_vec())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        fingerprint.sequence(b"concrete-paths", paths.len());
        for path in paths {
            fingerprint.field(b"concrete-path", &path);
        }
    }

    fn external_digest(&mut self, input: &LoweredExternalInput) -> anyhow::Result<ActionDigest> {
        if let Some(digest) = self.external_digests.get(input) {
            return Ok(*digest);
        }
        let path = input.path();
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("failed to inspect action input {}", path.display()))?;
        let digest = match input {
            LoweredExternalInput::File(_) if metadata.is_file() => self.digest_file(path)?,
            LoweredExternalInput::File(_) => {
                bail!("action input is not a regular file: {}", path.display())
            }
            LoweredExternalInput::StandardLibraryInterfaces(_) if metadata.is_dir() => {
                self.digest_stdlib_interfaces(path)?
            }
            LoweredExternalInput::StandardLibraryInterfaces(_) => bail!(
                "standard-library interface input is not a directory: {}",
                path.display()
            ),
        };
        self.external_digests.insert(input.clone(), digest);
        Ok(digest)
    }

    fn digest_stdlib_interfaces(&mut self, root: &Path) -> anyhow::Result<ActionDigest> {
        let mut pending = vec![(PathBuf::new(), Vec::<PathBuf>::new())];
        let mut interfaces = Vec::new();
        while let Some((relative_dir, mut ancestors)) = pending.pop() {
            let directory = root.join(&relative_dir);
            let canonical = std::fs::canonicalize(&directory).with_context(|| {
                format!(
                    "failed to resolve standard-library directory {}",
                    directory.display()
                )
            })?;
            if ancestors.contains(&canonical) {
                continue;
            }
            ancestors.push(canonical);

            for entry in std::fs::read_dir(&directory).with_context(|| {
                format!(
                    "failed to read standard-library directory {}",
                    directory.display()
                )
            })? {
                let entry = entry?;
                let relative_path = relative_dir.join(entry.file_name());
                let file_type = entry.file_type()?;
                let target_type = if file_type.is_symlink() {
                    match std::fs::metadata(entry.path()) {
                        Ok(metadata) => Some(metadata.file_type()),
                        Err(error) if error.kind() == ErrorKind::NotFound => None,
                        Err(error) => return Err(error.into()),
                    }
                } else {
                    Some(file_type)
                };
                if target_type.as_ref().is_some_and(|kind| kind.is_dir()) {
                    pending.push((relative_path, ancestors.clone()));
                } else if target_type.as_ref().is_some_and(|kind| kind.is_file())
                    && relative_path.extension() == Some(OsStr::new("mi"))
                {
                    interfaces.push(relative_path);
                } else if !file_type.is_symlink() && !file_type.is_file() {
                    bail!(
                        "standard-library directory contains an unsupported entry: {}",
                        entry.path().display()
                    );
                }
            }
        }

        interfaces.sort();
        let mut fingerprint = FingerprintHasher::new(b"moon-stdlib-interface-tree-v1");
        fingerprint.sequence(b"interfaces", interfaces.len());
        for relative_path in interfaces {
            fingerprint.field(b"path", path_bytes(&relative_path));
            fingerprint.field(
                b"content",
                self.digest_file(&root.join(relative_path))?.as_bytes(),
            );
        }
        Ok(fingerprint.finish())
    }

    fn digest_file(&mut self, path: &Path) -> anyhow::Result<ActionDigest> {
        if let Some(digest) = self.file_digests.get(path) {
            return Ok(*digest);
        }
        let mut file = File::open(path)
            .with_context(|| format!("failed to open action input {}", path.display()))?;
        let mut hasher = Hasher::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .with_context(|| format!("failed to read action input {}", path.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let digest = ActionDigest(*hasher.finalize().as_bytes());
        self.file_digests.insert(path.to_owned(), digest);
        Ok(digest)
    }
}

fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

fn normalized_environment(
    inherited: &[(OsString, OsString)],
    action: &[(String, String)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut normalized = BTreeMap::new();
    for (name, value) in inherited {
        normalized.insert(
            environment_name(name.as_encoded_bytes()),
            value.as_encoded_bytes().to_vec(),
        );
    }
    for (name, value) in action {
        normalized.insert(environment_name(name.as_bytes()), value.as_bytes().to_vec());
    }
    normalized.into_iter().collect()
}

fn environment_name(name: &[u8]) -> Vec<u8> {
    if cfg!(windows) {
        name.iter().map(u8::to_ascii_uppercase).collect()
    } else {
        name.to_vec()
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

struct FingerprintHasher {
    hasher: Hasher,
}

impl FingerprintHasher {
    fn new(domain: &[u8]) -> Self {
        let mut fingerprint = Self {
            hasher: Hasher::new(),
        };
        fingerprint.field(b"domain", domain);
        fingerprint
    }

    fn field(&mut self, name: &[u8], value: &[u8]) {
        self.bytes(name);
        self.bytes(value);
    }

    fn sequence(&mut self, name: &[u8], len: usize) {
        self.field(name, &(len as u64).to_le_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.hasher.update(&(value.len() as u64).to_le_bytes());
        self.hasher.update(value);
    }

    fn finish(self) -> ActionDigest {
        ActionDigest(*self.hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
    };

    use moonbuild_rupes_recta::model::TargetKind;
    use tempfile::TempDir;

    use super::*;

    fn product(producer: Option<usize>, path: impl Into<PathBuf>) -> CanonicalProduct {
        CanonicalProduct {
            producer,
            logical: LogicalProduct {
                kind: b"package-interface",
                target_kind: Some(TargetKind::Source),
                index: None,
                path: None,
            },
            paths: vec![path.into()],
        }
    }

    fn action(
        dependencies: Vec<CanonicalProduct>,
        external_inputs: Vec<LoweredExternalInput>,
        output: impl Into<PathBuf>,
    ) -> CanonicalAction {
        let output = output.into();
        CanonicalAction {
            dependencies,
            external_inputs,
            outputs: vec![product(None, output.clone())],
            command: CanonicalCommand {
                args: vec![
                    "/toolchain/bin/moonc".to_string(),
                    "check".to_string(),
                    "-o".to_string(),
                    output.display().to_string(),
                ],
                execution: CanonicalExecution::Inline {
                    command: format!("/toolchain/bin/moonc check -o {}", output.display()),
                },
                cwd: None,
                environment: Vec::new(),
            },
            cache_eligible: true,
        }
    }

    fn context(root: &Path) -> ActionIdentityContext {
        ActionIdentityContext::new(root, vec![(OsString::from("LANG"), OsString::from("C"))])
    }

    #[test]
    fn physical_work_paths_are_part_of_identity() {
        let root = TempDir::new().unwrap();
        let source_root = root.path().join("stable/src");
        let toolchain_root = root.path().join("stable/toolchain");
        fs::create_dir_all(&source_root).unwrap();
        fs::write(source_root.join("lib.mbt"), "pub fn answer() -> Int { 42 }").unwrap();

        let make_action = |private_work_root: &Path| {
            let mut action = action(
                Vec::new(),
                vec![LoweredExternalInput::File(source_root.join("lib.mbt"))],
                private_work_root.join("lib.mi"),
            );
            action.command.args = vec![
                toolchain_root.join("bin/moonc").display().to_string(),
                "check".to_string(),
                format!("-pkg-sources=pkg:{}", source_root.display()),
                "-o".to_string(),
                private_work_root.join("lib.mi").display().to_string(),
            ];
            action.command.cwd = Some(source_root.clone());
            action.command.execution = CanonicalExecution::ResponseFile {
                command: format!(
                    "{} @{}",
                    toolchain_root.join("bin/moonc").display(),
                    private_work_root.join("lib.rsp").display()
                ),
                path: private_work_root.join("lib.rsp"),
                content: format!("-o {}\n", private_work_root.join("lib.mi").display()),
            };
            action
        };

        let first_work_root = root.path().join("invocation-a");
        let second_work_root = root.path().join("invocation-b");
        let inherited_environment = vec![(OsString::from("LANG"), OsString::from("C"))];
        let first_context = ActionIdentityContext::new(&source_root, inherited_environment.clone());
        let second_context = ActionIdentityContext::new(&source_root, inherited_environment);
        let first_identity =
            compute_canonical_actions(&[make_action(&first_work_root)], &first_context).unwrap();
        let repeated_identity =
            compute_canonical_actions(&[make_action(&first_work_root)], &first_context).unwrap();
        let second_identity =
            compute_canonical_actions(&[make_action(&second_work_root)], &second_context).unwrap();

        assert_eq!(first_identity, repeated_identity);
        assert_ne!(first_identity, second_identity);
    }

    #[test]
    fn physical_external_input_path_is_part_of_identity() {
        let root = TempDir::new().unwrap();
        let first_source = root.path().join("source-a/lib.mbt");
        let second_source = root.path().join("source-b/lib.mbt");
        fs::create_dir_all(first_source.parent().unwrap()).unwrap();
        fs::create_dir_all(second_source.parent().unwrap()).unwrap();
        fs::write(&first_source, "pub fn answer() -> Int { 42 }").unwrap();
        fs::write(&second_source, "pub fn answer() -> Int { 42 }").unwrap();

        let output = root.path().join("_build/lib.mi");
        let first = action(
            Vec::new(),
            vec![LoweredExternalInput::File(first_source)],
            &output,
        );
        let second = action(
            Vec::new(),
            vec![LoweredExternalInput::File(second_source)],
            output,
        );

        assert_ne!(
            compute_canonical_actions(&[first], &context(root.path())).unwrap(),
            compute_canonical_actions(&[second], &context(root.path())).unwrap()
        );
    }

    #[test]
    fn lexically_distinct_physical_paths_are_not_normalized() {
        let root = TempDir::new().unwrap();
        let direct_output = root.path().join("_build/lib.mi");
        let dotted_output = root.path().join("_build/./lib.mi");
        assert_ne!(path_bytes(&direct_output), path_bytes(&dotted_output));

        assert_ne!(
            compute_canonical_actions(
                &[action(Vec::new(), Vec::new(), direct_output)],
                &context(root.path()),
            )
            .unwrap(),
            compute_canonical_actions(
                &[action(Vec::new(), Vec::new(), dotted_output)],
                &context(root.path()),
            )
            .unwrap()
        );
    }

    #[test]
    fn effective_working_directory_is_part_of_identity() {
        let root = TempDir::new().unwrap();
        let private_work_root = root.path().join("_build");
        let action = action(Vec::new(), Vec::new(), private_work_root.join("lib.mi"));
        let environment = vec![(OsString::from("LANG"), OsString::from("C"))];
        let first_context =
            ActionIdentityContext::new(root.path().join("cwd-a"), environment.clone());
        let second_context = ActionIdentityContext::new(root.path().join("cwd-b"), environment);

        assert_ne!(
            compute_canonical_actions(std::slice::from_ref(&action), &first_context).unwrap(),
            compute_canonical_actions(std::slice::from_ref(&action), &second_context).unwrap()
        );

        let mut explicit = action;
        explicit.command.cwd = Some(root.path().join("explicit-cwd"));
        assert_eq!(
            compute_canonical_actions(std::slice::from_ref(&explicit), &first_context).unwrap(),
            compute_canonical_actions(&[explicit], &second_context).unwrap()
        );
    }

    #[test]
    fn producer_identity_propagates_without_hashing_action_numbers() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("dep.mbt"), "let value = 1").unwrap();
        let producer = action(
            Vec::new(),
            vec![LoweredExternalInput::File(root.path().join("dep.mbt"))],
            root.path().join("_build/dep.mi"),
        );
        let consumer = action(
            vec![product(Some(0), root.path().join("_build/dep.mi"))],
            Vec::new(),
            root.path().join("_build/main.mi"),
        );
        let original =
            compute_canonical_actions(&[producer, consumer], &context(root.path())).unwrap();

        let producer = action(
            Vec::new(),
            vec![LoweredExternalInput::File(root.path().join("dep.mbt"))],
            root.path().join("_build/dep.mi"),
        );
        let consumer = action(
            vec![product(Some(1), root.path().join("_build/dep.mi"))],
            Vec::new(),
            root.path().join("_build/main.mi"),
        );
        let reordered =
            compute_canonical_actions(&[consumer, producer], &context(root.path())).unwrap();

        assert_eq!(original[0].digest(), reordered[1].digest());
        assert_eq!(original[1].digest(), reordered[0].digest());

        fs::write(root.path().join("dep.mbt"), "let value = 2").unwrap();
        let producer = action(
            Vec::new(),
            vec![LoweredExternalInput::File(root.path().join("dep.mbt"))],
            root.path().join("_build/dep.mi"),
        );
        let consumer = action(
            vec![product(Some(0), root.path().join("_build/dep.mi"))],
            Vec::new(),
            root.path().join("_build/main.mi"),
        );
        let changed =
            compute_canonical_actions(&[producer, consumer], &context(root.path())).unwrap();

        assert_ne!(original[0].digest(), changed[0].digest());
        assert_ne!(original[1].digest(), changed[1].digest());
    }

    #[test]
    fn effective_environment_is_ordered_and_action_values_override_inherited_values() {
        let root = TempDir::new().unwrap();
        let output = root.path().join("_build/lib.mi");
        let mut action = action(Vec::new(), Vec::new(), &output);
        action.command.environment = vec![("MODE".to_string(), "action".to_string())];

        let first = ActionIdentityContext::new(
            root.path(),
            vec![
                (OsString::from("Z"), OsString::from("last")),
                (OsString::from("MODE"), OsString::from("inherited")),
                (OsString::from("A"), OsString::from("first")),
            ],
        );
        let reordered = ActionIdentityContext::new(
            root.path(),
            vec![
                (OsString::from("A"), OsString::from("first")),
                (OsString::from("MODE"), OsString::from("ignored")),
                (OsString::from("Z"), OsString::from("last")),
            ],
        );
        let changed = ActionIdentityContext::new(
            root.path(),
            vec![
                (OsString::from("A"), OsString::from("changed")),
                (OsString::from("MODE"), OsString::from("ignored")),
                (OsString::from("Z"), OsString::from("last")),
            ],
        );

        let original = compute_canonical_actions(&[action.clone()], &first).unwrap();
        assert_eq!(
            original,
            compute_canonical_actions(&[action.clone()], &reordered).unwrap()
        );
        assert_ne!(
            original,
            compute_canonical_actions(&[action], &changed).unwrap()
        );
    }

    #[test]
    fn unordered_inputs_are_canonical_but_argument_boundaries_are_semantic() {
        let root = TempDir::new().unwrap();
        let first_input = root.path().join("first.mbt");
        let second_input = root.path().join("second.mbt");
        fs::write(&first_input, "first").unwrap();
        fs::write(&second_input, "second").unwrap();

        let first_producer = action(
            Vec::new(),
            vec![LoweredExternalInput::File(first_input.clone())],
            root.path().join("_build/first.mi"),
        );
        let second_producer = action(
            Vec::new(),
            vec![LoweredExternalInput::File(second_input.clone())],
            root.path().join("_build/second.mi"),
        );
        let mut consumer = action(
            vec![
                product(Some(0), root.path().join("_build/first.mi")),
                product(Some(1), root.path().join("_build/second.mi")),
            ],
            vec![
                LoweredExternalInput::File(first_input.clone()),
                LoweredExternalInput::File(second_input.clone()),
            ],
            root.path().join("_build/main.mi"),
        );
        consumer.command.args = vec!["tool".into(), "ab".into(), "c".into()];
        let original = compute_canonical_actions(
            &[
                first_producer.clone(),
                second_producer.clone(),
                consumer.clone(),
            ],
            &context(root.path()),
        )
        .unwrap();

        consumer.dependencies.reverse();
        consumer.external_inputs.reverse();
        let reordered = compute_canonical_actions(
            &[first_producer, second_producer, consumer.clone()],
            &context(root.path()),
        )
        .unwrap();
        assert_eq!(original[2].digest(), reordered[2].digest());

        consumer.command.args = vec!["tool".into(), "a".into(), "bc".into()];
        let different_arguments = compute_canonical_actions(
            &[
                action(
                    Vec::new(),
                    vec![LoweredExternalInput::File(first_input)],
                    root.path().join("_build/first.mi"),
                ),
                action(
                    Vec::new(),
                    vec![LoweredExternalInput::File(second_input)],
                    root.path().join("_build/second.mi"),
                ),
                consumer,
            ],
            &context(root.path()),
        )
        .unwrap();
        assert_ne!(original[2].digest(), different_arguments[2].digest());
    }

    #[test]
    fn selected_transport_command_is_part_of_the_identity() {
        let root = TempDir::new().unwrap();
        let mut action = action(Vec::new(), Vec::new(), root.path().join("_build/lib.mi"));
        let original = compute_canonical_actions(&[action.clone()], &context(root.path())).unwrap();

        action.command.execution = CanonicalExecution::Inline {
            command: "different executor command".to_string(),
        };
        let changed = compute_canonical_actions(&[action], &context(root.path())).unwrap();

        assert_ne!(original[0].digest(), changed[0].digest());
    }

    #[test]
    fn standard_library_identity_observes_only_the_mi_tree() {
        let root = TempDir::new().unwrap();
        let stdlib = root.path().join("stdlib");
        fs::create_dir_all(stdlib.join("pkg")).unwrap();
        fs::write(stdlib.join("pkg/pkg.mi"), "interface one").unwrap();
        fs::write(stdlib.join("README.md"), "one").unwrap();
        let action = action(
            Vec::new(),
            vec![LoweredExternalInput::StandardLibraryInterfaces(
                stdlib.clone(),
            )],
            root.path().join("_build/lib.mi"),
        );
        let original =
            compute_canonical_actions(std::slice::from_ref(&action), &context(root.path()))
                .unwrap();

        fs::write(stdlib.join("README.md"), "two").unwrap();
        assert_eq!(
            original,
            compute_canonical_actions(std::slice::from_ref(&action), &context(root.path()))
                .unwrap()
        );

        fs::write(stdlib.join("pkg/pkg.mi"), "interface two").unwrap();
        assert_ne!(
            original,
            compute_canonical_actions(&[action], &context(root.path())).unwrap()
        );
    }

    #[test]
    fn derived_all_packages_file_is_not_a_content_input() {
        let root = TempDir::new().unwrap();
        let all_packages = root.path().join("_build/all_pkgs.json");
        fs::create_dir_all(all_packages.parent().unwrap()).unwrap();
        fs::write(&all_packages, r#"{"unrelated":"one"}"#).unwrap();
        let mut action = action(Vec::new(), Vec::new(), root.path().join("_build/lib.mi"));
        action
            .command
            .args
            .extend(["-all-pkgs".to_string(), all_packages.display().to_string()]);
        let original = compute_canonical_actions(&[action.clone()], &context(root.path())).unwrap();

        fs::write(&all_packages, r#"{"unrelated":"two"}"#).unwrap();
        assert_eq!(
            original,
            compute_canonical_actions(&[action], &context(root.path())).unwrap()
        );
    }

    #[test]
    fn ineligible_execution_makes_its_consumers_ineligible() {
        let root = TempDir::new().unwrap();
        let unmodeled_directory = root.path().join("unmodeled-directory");
        fs::create_dir(&unmodeled_directory).unwrap();
        let mut producer = action(
            Vec::new(),
            vec![LoweredExternalInput::File(unmodeled_directory)],
            root.path().join("_build/generated.mbt"),
        );
        producer.cache_eligible = false;
        let consumer = action(
            vec![product(Some(0), root.path().join("_build/generated.mbt"))],
            Vec::new(),
            root.path().join("_build/lib.mi"),
        );

        let identities =
            compute_canonical_actions(&[producer, consumer], &context(root.path())).unwrap();
        assert!(!identities[0].is_cacheable());
        assert!(!identities[1].is_cacheable());
    }
}
