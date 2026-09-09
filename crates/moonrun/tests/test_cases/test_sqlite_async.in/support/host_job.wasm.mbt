// Copyright 2025 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
