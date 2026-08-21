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

use crate::{async_policy, source_map, v8_backend};
use anyhow::Context;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Process-wide engine configuration for the current V8-backed implementation.
///
/// V8 flags are process-global and must be selected before the first run. All
/// [`Engine`] values in one process therefore need the same configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineConfig {
    pub(crate) stack_size: Option<usize>,
}

impl EngineConfig {
    pub fn with_stack_size(mut self, stack_size: usize) -> Self {
        self.stack_size = Some(stack_size);
        self
    }
}

/// Configuration for one MoonBit Wasm run.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    pub(crate) args: Vec<String>,
    pub(crate) no_stack_trace: bool,
    pub(crate) test_args: Option<String>,
    pub(crate) policy_file: Option<PathBuf>,
}

impl RunOptions {
    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn without_stack_trace(mut self) -> Self {
        self.no_stack_trace = true;
        self
    }

    pub fn with_test_args(mut self, test_args: impl Into<String>) -> Self {
        self.test_args = Some(test_args.into());
        self
    }

    pub fn with_policy_file(mut self, policy_file: impl Into<PathBuf>) -> Self {
        self.policy_file = Some(policy_file.into());
        self
    }
}

/// The observable result of one MoonBit Wasm run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Exited(i32),
    KilledBySignal(i32),
}

struct ModuleData {
    name: String,
    compiled: v8_backend::CompiledModule,
    source_map: Option<String>,
}

/// An immutable Wasm module compiled by an [`Engine`].
///
/// Loading compiles the file once against the Engine. Cloning a Module reuses
/// that compiled representation, and each call to [`Engine::run`] creates
/// fresh per-run guest and host state from it.
#[derive(Clone)]
pub struct Module(Arc<ModuleData>);

impl Module {
    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }

    pub(crate) fn compiled(&self) -> &v8_backend::CompiledModule {
        &self.0.compiled
    }

    pub(crate) fn source_map(&self) -> Option<&str> {
        self.0.source_map.as_deref()
    }
}

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.0.name)
            .finish_non_exhaustive()
    }
}

/// Process-shared Moonrun execution engine.
///
/// An Engine loads reusable modules and executes each isolated run synchronously
/// on the calling thread. It does not create threads or retain run lifecycle
/// state; callers choose execution placement and manage lifecycle.
///
/// The current implementation still uses process stdio, environment, working
/// directory, and signal compatibility behavior. Those remain shared,
/// process-scoped dependencies to extract in later changes.
#[derive(Clone, Debug)]
pub struct Engine {
    config: EngineConfig,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self { config }
    }

    /// Compile Wasm bytes into a reusable immutable Module.
    pub fn compile(
        &self,
        name: impl Into<String>,
        bytes: impl AsRef<[u8]>,
    ) -> anyhow::Result<Module> {
        let name = name.into();
        let compiled = v8_backend::compile(&self.config, bytes.as_ref())
            .with_context(|| format!("failed to compile `{name}`"))?;
        Ok(Module(Arc::new(ModuleData {
            name,
            compiled,
            source_map: None,
        })))
    }

    /// Compile a Wasm file into a reusable immutable Module.
    pub fn load_file(&self, file: impl AsRef<Path>) -> anyhow::Result<Module> {
        let file = file.as_ref();
        if !file.exists() {
            anyhow::bail!("no such file");
        }
        if file.extension().and_then(|extension| extension.to_str()) != Some("wasm") {
            anyhow::bail!("Unsupported file type");
        }
        let bytes = std::fs::read(file).context("failed to read Wasm file")?;
        let name = file.to_string_lossy().into_owned();
        let compiled = v8_backend::compile(&self.config, &bytes)
            .with_context(|| format!("failed to compile `{name}`"))?;
        Ok(Module(Arc::new(ModuleData {
            name,
            compiled,
            source_map: source_map::load(file, &bytes),
        })))
    }

    /// Execute one isolated run synchronously on the calling thread.
    pub fn run(&self, module: &Module, options: RunOptions) -> anyhow::Result<RunOutcome> {
        let async_policy = Arc::new(match options.policy_file.as_ref() {
            Some(path) => async_policy::AsyncPolicy::from_file(path).context(
                "failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes",
            )?,
            None => async_policy::AsyncPolicy::allow_all(),
        });
        v8_backend::run(
            &self.config,
            module.name(),
            module.compiled(),
            module.source_map(),
            options,
            async_policy,
        )
    }

    /// Load and synchronously execute one Wasm file.
    pub fn run_file(
        &self,
        file: impl AsRef<Path>,
        options: RunOptions,
    ) -> anyhow::Result<RunOutcome> {
        let module = self.load_file(file)?;
        self.run(&module, options)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_module_keeps_source_map_after_files_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let wasm_path = dir.path().join("main.wasm");
        let map_dir = dir.path().join("maps");
        let map_path = map_dir.join("original.map");
        let source_map = r#"{"version":3,"sources":["main.mbt"],"mappings":"AAAA"}"#;
        let section_name = b"sourceMappingURL";
        let map_name = b"maps/original.map";
        let section_len = 1 + section_name.len() + 1 + map_name.len();
        let wasm = [
            b"\0asm\x01\0\0\0".as_slice(),
            &[0, section_len as u8],
            &[section_name.len() as u8],
            section_name,
            &[map_name.len() as u8],
            map_name,
        ]
        .concat();
        std::fs::create_dir(map_dir).unwrap();
        std::fs::write(&wasm_path, wasm).unwrap();
        std::fs::write(&map_path, source_map).unwrap();

        let module = Engine::default().load_file(&wasm_path).unwrap();
        std::fs::remove_file(wasm_path).unwrap();
        std::fs::remove_file(map_path).unwrap();

        assert_eq!(module.source_map(), Some(source_map));
    }
}
