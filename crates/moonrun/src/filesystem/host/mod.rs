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

//! Runtime filesystem state and authorized operations.
//! Pathname, directory-relative, and acquired-resource operations share HostFs.

mod directory;
mod path;
mod remove_tree;
mod resource;
mod temp_dir;
#[cfg(test)]
mod tests;

pub(crate) use directory::Directory;
#[cfg(test)]
pub(crate) use temp_dir::PORTED_SYMBOLS as TEMP_DIR_PORTED_SYMBOLS;

use crate::filesystem::{FsAuthorizer, FsOperation};
use crate::policy::AccessRights;
use crate::runtime::{Env, WorkingDirectory};
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

/// Preserve policy denial separately from OS errors for each caller's ABI.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FsError {
    #[error("filesystem access denied by policy")]
    Denied,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Engine-neutral filesystem operations that enforce the runtime policy
/// before accessing the operating system's filesystem.
pub(crate) struct HostFs {
    authorizer: Box<dyn FsAuthorizer>,
    environment: Arc<Env>,
    working_directory: Arc<WorkingDirectory>,
}

// Domain executors require an explicit filesystem even in scheduler-only tests.
// Production construction must continue to supply Runtime state and policy.
#[cfg(test)]
impl Default for HostFs {
    fn default() -> Self {
        Self::with_policy(
            None,
            Arc::new(Env::ambient()),
            Arc::new(WorkingDirectory::Ambient),
        )
    }
}

impl HostFs {
    pub(crate) fn resolve_path(&self, path: &OsStr) -> OsString {
        self.working_directory.resolve_path(path)
    }

    pub(crate) fn new(
        authorizer: impl FsAuthorizer + 'static,
        environment: Arc<Env>,
        working_directory: Arc<WorkingDirectory>,
    ) -> Self {
        Self {
            authorizer: Box::new(authorizer),
            environment,
            working_directory,
        }
    }

    pub(crate) fn authorize(&self, operation: FsOperation<'_>) -> Result<AccessRights, FsError> {
        let granted = self.authorizer.authorize(operation)?;
        // An open grant is retained by async handles. A partial grant must
        // never turn an explicit open into a successful, weaker one.
        if matches!(operation, FsOperation::Open { .. }) {
            operation.check_grant(granted)?;
        }
        Ok(granted)
    }

    pub(crate) fn temp_dir(&self) -> Result<OsString, FsError> {
        temp_dir::resolve(&self.environment)
    }
}
