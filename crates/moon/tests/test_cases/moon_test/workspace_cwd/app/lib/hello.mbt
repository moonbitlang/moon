pub fn hello() -> String {
  "app"
}

#cfg(target="wasm")
test "runs app tests with the workspace policy" {
  assert_eq(hello(), "app")
}
