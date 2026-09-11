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

pub fn line_col_to_byte_idx(
    line_index: &line_index::LineIndex,
    line: u32,
    col: u32,
) -> Option<usize> {
    let offset = line_index.offset(line_index.to_utf8(
        line_index::WideEncoding::Utf32,
        line_index::WideLineCol { line, col },
    )?)?;
    Some(usize::from(offset))
}

pub trait StringExt {
    fn replace_crlf_to_lf(&self) -> String;
}

impl StringExt for str {
    fn replace_crlf_to_lf(&self) -> String {
        self.replace("\r\n", "\n")
    }
}
