fn hello() -> String {
  "Hello, world!"
}

test "hello_test" {
  assert_eq!(hello(), "Goodbye, world!")
}
