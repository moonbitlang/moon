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
  let args = @env.args()
  guard args.length() == 3 else {
    abort("expected readiness port and response arguments")
  }
  let readiness_port = @string.parse_int(args[1])
  let response = args[2]
  let server = @http.Server(@socket.Addr::parse("127.0.0.1:0"))
  let readiness = @socket.Tcp::connect(
    @socket.Addr::parse("127.0.0.1:\{readiness_port}"),
  )
  readiness.write("\{server.addr.port()}")
  readiness.close()
  let (conn, _) = server.accept()
  defer conn.close()
  defer server.close()
  ignore(conn.read_request())
  conn
  ..send_response(200, "OK", extra_headers={
    "Content-Type": "text/plain",
    "Connection": "close",
  })
  ..write(response)
  .end_response()
}
