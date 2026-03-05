test "web" {
  if web_banner() != "shared-jsdep" {
    fail("web_banner() mismatch")
  }
}
