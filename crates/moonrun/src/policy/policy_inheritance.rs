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

//! Immutable restrictions shared by virtual children and host domains.

use std::sync::Arc;

use super::{FsPolicy, NetPolicy, Policy, ProcessPolicy};

#[derive(Clone, Debug)]
pub(crate) struct PolicyInheritance(pub(super) Arc<ResolvedRestrictions>);

/// In-process children share resolved rules; only external children decode a transfer.
#[derive(Debug)]
pub(crate) enum InheritedPolicyInput {
    Resolved(PolicyInheritance),
    Transfer(Vec<u8>),
}

/// Constructed from validated domain rules. Source loading resolves filesystem
/// roots; transfer decoding validates their representation without resolving them.
#[derive(Debug)]
pub(super) struct ResolvedRestrictions {
    pub fs: FsPolicy,
    pub net: NetPolicy,
    pub process: ProcessPolicy,
}

impl PolicyInheritance {
    pub(super) fn new(fs: FsPolicy, net: NetPolicy, process: ProcessPolicy) -> Self {
        Self(Arc::new(ResolvedRestrictions { fs, net, process }))
    }

    pub(crate) fn policy(&self) -> Policy {
        Policy {
            fs: Some(self.0.fs.clone()),
            net: Some(self.0.net.clone()),
            process: Some(self.0.process.clone()),
            policy_inheritance: Some(self.clone()),
        }
    }
}
