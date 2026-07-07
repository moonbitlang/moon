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
