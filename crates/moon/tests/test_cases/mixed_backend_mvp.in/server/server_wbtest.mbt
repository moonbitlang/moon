test "server" {
  if listen_addr() != @shared.server_addr() {
    fail("listen_addr() mismatch")
  }
  if api_path() != @shared.hello_path() {
    fail("api_path() mismatch")
  }
}
