test "shared" {
  if app_title() != "MoonBuild Mixed Backend MVP" {
    fail("app_title() mismatch")
  }
  if api_url() != "http://127.0.0.1:3001/api/hello" {
    fail("api_url() mismatch")
  }
}
