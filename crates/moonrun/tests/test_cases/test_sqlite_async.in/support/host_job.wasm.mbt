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

// Companion entry point for moonbitlang/async. Kept outside the submodule so
// the integration harness can apply it to a disposable copy of the pinned
// upstream sources. It reuses the existing worker queue and completion path.

///|
#cfg(target="wasm")
pub async fn perform_host_job(handle : UInt64, context~ : String) -> Int {
  // The caller owns the host Job and releases it after copying/taking results.
  // Default non-interrupting cancellation waits for submitted work to finish.
  perform_job_in_worker(Job::Job(JobHandle(handle)), context~)
}
