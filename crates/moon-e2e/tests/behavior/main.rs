// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use cucumber::World as _;
use moon_test_util::test_dir::TestDir;
use snapbox::cmd::OutputAssert;

mod clean;
mod command;
mod conditional_compilation;
mod files;
mod library_build;
mod package_interfaces;
mod package_selection;
mod packaging;
mod preview;
mod snapshot_updates;
mod support;

const TEST_CASES_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../moon/tests/test_cases");

#[derive(Default, cucumber::World)]
struct World {
    cwd: Option<PathBuf>,
    target_dir: Option<PathBuf>,
    // Own the private source copy or build directory until the scenario ends.
    _workspace: Option<TestDir>,
    output: Option<OutputAssert>,
    package_index: Option<serde_json::Value>,
    source_files_before_update: BTreeMap<String, Vec<u8>>,
}

impl fmt::Debug for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World")
            .field("project", &self.cwd)
            .field("has_command_output", &self.output.is_some())
            .finish()
    }
}

impl World {
    fn copy_case(&mut self, case: &str) {
        let workspace = TestDir::from_case_root(TEST_CASES_ROOT, case, true);
        self.cwd = Some(workspace.as_ref().to_owned());
        self.target_dir = None;
        self._workspace = Some(workspace);
    }

    fn project_path(&self) -> &Path {
        self.cwd.as_deref().expect("Given a project")
    }

    fn command(&self) -> snapbox::cmd::Command {
        let command = support::moon_cmd(self.project_path());
        match &self.target_dir {
            Some(target_dir) => command.arg("--target-dir").arg(target_dir),
            None => command,
        }
    }
}

fn main() {
    // Steps use synchronous processes and filesystem operations. Cucumber's
    // existing executor is sufficient; no Tokio reactor or worker pool is needed.
    futures_executor::block_on(
        World::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(Path::new(env!("CARGO_MANIFEST_DIR")).join("features")),
    );
}
