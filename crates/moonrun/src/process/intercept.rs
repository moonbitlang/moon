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

//! Resolve an authorized guest moonx invocation into a supervised child.

//! Command recognition and validation, before execution-specific preparation.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use super::SpawnOptions;
use super::spawn::SpawnRequest;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::ResourceRef;
use crate::runtime::Env;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CommandInterception {
    #[default]
    Disabled,
    Moonx,
}

pub(super) enum Command {
    Native {
        request: SpawnRequest,
        inherits_policy: bool,
    },
    Moonx(MoonInvocation),
}

/// A recognized moonx invocation with validated child inputs. Execution only
/// resolves its image, binds resources, and admits the child.
pub(super) struct MoonInvocation {
    pub(super) args: Vec<OsString>,
    pub(super) environment: Vec<(OsString, OsString)>,
    pub(super) cwd: PathBuf,
    pub(super) stdio: [Option<ResourceRef>; 3],
    pub(super) options: SpawnOptions,
}

impl CommandInterception {
    pub(super) fn select(
        self,
        request: SpawnRequest,
        #[cfg(windows)] mut arguments: Vec<OsString>,
    ) -> AsyncHostResult<Command> {
        if self == Self::Disabled {
            #[cfg(unix)]
            let inherits_policy = is_policy_recipient(&request.path);
            #[cfg(windows)]
            let inherits_policy = is_policy_recipient(&arguments[0]);
            return Ok(Command::Native {
                request,
                inherits_policy,
            });
        }
        #[cfg(unix)]
        let (program, args, environment) = {
            use std::os::unix::ffi::{OsStrExt, OsStringExt};
            use std::path::Path;

            let environment = request
                .env
                .into_iter()
                .map(|entry| {
                    let bytes = entry.into_vec();
                    let split = bytes
                        .iter()
                        .position(|byte| *byte == b'=')
                        .ok_or(AsyncHostError::Inval)?;
                    Ok((
                        OsStr::from_bytes(&bytes[..split]).to_owned(),
                        OsStr::from_bytes(&bytes[split + 1..]).to_owned(),
                    ))
                })
                .collect::<AsyncHostResult<Vec<_>>>()?;
            // Moon selects moonx by argv[0]; preserve rejection of forged argv[0].
            if request
                .args
                .first()
                .and_then(|arg| Path::new(arg).file_name())
                != Path::new(&request.path).file_name()
            {
                return Err(AsyncHostError::Inval);
            }
            (
                request.path,
                request.args.into_iter().skip(1).collect::<Vec<_>>(),
                environment,
            )
        };
        #[cfg(windows)]
        let (program, args, environment) = {
            use std::os::windows::ffi::OsStringExt;

            let program = arguments.remove(0);
            let environment = request
                .env
                .split(|unit| *unit == 0)
                .take_while(|entry| !entry.is_empty())
                .map(|entry| {
                    // Hidden drive-directory entries start with '='.
                    let split = entry
                        .iter()
                        .enumerate()
                        .skip(1)
                        .find(|(_, unit)| **unit == b'=' as u16)
                        .map(|(index, _)| index)
                        .ok_or(AsyncHostError::Inval)?;
                    Ok((
                        OsString::from_wide(&entry[..split]),
                        OsString::from_wide(&entry[split + 1..]),
                    ))
                })
                .collect::<AsyncHostResult<Vec<_>>>()?;
            (program, arguments, environment)
        };
        if program.as_encoded_bytes().contains(&0)
            || args.iter().any(|arg| arg.as_encoded_bytes().contains(&0))
        {
            return Err(AsyncHostError::Inval);
        }
        let environment = Env::owned(environment)
            .map_err(|_| AsyncHostError::Inval)?
            .entries();
        // Malformed payloads remain spawn errors even for unsupported commands.
        // Recognition must precede resource binding or artifact preparation.
        if !moonutil::constants::is_moonx_executable(&program) {
            #[cfg(unix)]
            return Err(AsyncHostError::Native(libc::ENOTSUP));
            #[cfg(windows)]
            return Err(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32,
            ));
        }
        let cwd = PathBuf::from(request.cwd.ok_or(AsyncHostError::Inval)?);
        if cwd.as_os_str().is_empty() {
            return Err(AsyncHostError::Inval);
        }
        Ok(Command::Moonx(MoonInvocation {
            args,
            environment,
            cwd,
            stdio: request.stdio,
            options: request.options,
        }))
    }
}

// moonrun consumes inherited restrictions directly; moonx relays them to moonrun.
pub(super) fn is_policy_recipient(program: &OsStr) -> bool {
    moonutil::constants::is_moonx_executable(program)
        || moonutil::constants::is_moonrun_executable(program)
}
