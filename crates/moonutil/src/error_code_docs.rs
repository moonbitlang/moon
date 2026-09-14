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

use std::collections::HashMap;

// include the generated error code docs
include!(concat!(env!("OUT_DIR"), "/error_code_docs.rs"));

pub fn get_error_code_doc(error_code: &str) -> Option<&'static str> {
    ERROR_DOCS.get(error_code).copied()
}

pub fn get_all_error_code_docs() -> Vec<(&'static str, &'static str)> {
    let mut docs: Vec<_> = ERROR_DOCS.iter().map(|(code, doc)| (*code, *doc)).collect();
    docs.sort_by_key(|(code, _)| *code);
    docs
}

pub fn get_all_attribute_docs() -> Vec<(&'static str, &'static str)> {
    let mut docs: Vec<_> = ATTRIBUTE_DOCS
        .iter()
        .map(|(attribute, doc)| (*attribute, *doc))
        .collect();
    docs.sort_by_key(|(attribute, _)| *attribute);
    docs
}
