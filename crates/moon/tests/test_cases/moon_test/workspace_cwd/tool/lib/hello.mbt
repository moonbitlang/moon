pub fn hello() -> String {
  "tool"
}

#cfg(target="wasm")
test "runs tool tests with the workspace policy" {
  assert_eq(hello(), "tool")
}
