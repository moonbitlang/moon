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

#[cfg(target_os = "macos")]
mod condition;
#[cfg(windows)]
mod event;
#[cfg(target_os = "linux")]
mod signal;

#[cfg(target_os = "macos")]
pub(super) use condition::{WorkerThreadId, WorkerWakeup, cancel_running_worker};
#[cfg(windows)]
pub(super) use event::{WorkerThreadId, WorkerWakeup, cancel_running_worker};
#[cfg(target_os = "linux")]
pub(super) use signal::{WorkerThreadId, WorkerWakeup, cancel_running_worker};
