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

// Integration tests link the normal library, so its unit-test hook does not
// run here. Initialize the production V8 platform before libtest creates any
// workers, which must inherit Linux protection-key permissions from this thread.
// Tests selecting a different EngineConfig must stay in their own processes
// and initialize that configuration before creating their execution threads.
ctor_lite::ctor! {
    // SAFETY: this is the only V8 startup hook in this binary. The default
    // path uses initially unlocked OnceLock/Mutex globals (with no prior
    // panic), allocation, and V8's C++ API; none need Rust runtime setup.
    // Keep Rust arguments, env, stdio, and thread APIs out of this path.
    // Abort on returned errors without running Rust cleanup.
    unsafe fn initialize_test_engine() {
        if moonrun::initialize(moonrun::EngineConfig::default()).is_err() {
            std::process::abort();
        }
    }
}
