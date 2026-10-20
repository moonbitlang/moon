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

//! Interactive terminal prompts.

use std::io;

use dialoguer::{MultiSelect, console::Term};

/// Shows a vertical checkbox prompt on stderr and returns the selected indices.
///
/// Space toggles the highlighted item, `a` toggles every item, and Enter
/// confirms. Escape or `q` cancels and returns `None`. Callers must ensure
/// both the input and rendering streams are terminals.
pub fn multi_select(prompt: &str, items: &[String]) -> io::Result<Option<Vec<usize>>> {
    MultiSelect::new()
        .with_prompt(prompt)
        .items(items)
        .interact_on_opt(&Term::stderr())
        .map_err(|error| match error {
            dialoguer::Error::IO(error) => error,
        })
}
