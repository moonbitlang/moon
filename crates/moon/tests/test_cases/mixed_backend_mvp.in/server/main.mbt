pub fn listen_addr() -> String {
  @shared.server_addr()
}

pub fn api_path() -> String {
  @shared.hello_path()
}

async fn send_plain_text(
  conn : @http.ServerConnection,
  code : Int,
  reason : String,
  body : String,
) -> Unit {
  conn
  ..send_response(code, reason, extra_headers={
    "Content-Type": "text/plain; charset=utf-8",
    "Access-Control-Allow-Origin": "*",
  })
  ..write(body)
  .end_response()
}

pub async fn run_server() -> Unit {
  let server = @http.Server::new(@socket.Addr::parse(listen_addr()))
  println("listening on http://\{listen_addr()}")
  server.run_forever((request, _body, conn) => {
    match request.meth {
      Get => {
        if request.path == api_path() {
          send_plain_text(conn, 200, "OK", @shared.hello_message())
        } else {
          send_plain_text(conn, 404, "NotFound", "`\{request.path}` not found")
        }
      }
      _ => send_plain_text(conn, 405, "MethodNotAllowed", "only GET is supported")
    }
  })
}

async fn main {
  run_server()
}
