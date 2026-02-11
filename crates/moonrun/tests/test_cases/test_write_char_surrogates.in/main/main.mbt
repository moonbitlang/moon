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

fn write_char(fd : Int, c : Char) = "__moonbit_io_unstable" "write_char"

fn main {
  // Write UTF-16 surrogate halves directly to verify host runtime handling.
  write_char(1, Char::from_int(0xD83D))
  write_char(1, Char::from_int(0xDE04))
  write_char(1, Char::from_int(0xD83D))
  write_char(1, Char::from_int(0xDC4D))
  write_char(1, '\n')
}
