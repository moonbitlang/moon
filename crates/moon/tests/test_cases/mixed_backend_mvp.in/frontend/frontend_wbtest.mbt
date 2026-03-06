test "frontend" {
  if mount_id() != "app" {
    fail("mount_id() mismatch")
  }
  if frontend_banner() != @shared.app_title() {
    fail("frontend_banner() mismatch")
  }
}
