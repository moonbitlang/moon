test "server" {
  if server_banner() != "shared-nativedep" {
    fail("server_banner() mismatch")
  }
}
