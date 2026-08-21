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

///|
async fn main {
  try {
    let server = @socket.TcpServer(@socket.Addr::parse("0.0.0.0:0"))
    defer server.close()
    abort("listen unexpectedly bypassed bind policy")
  } catch {
    @os_error.OSError(_) as err if err.is_EACCES() => {
      println("listen denied")
    }
    err => abort("unexpected listen error: \{err}")
  }
}
