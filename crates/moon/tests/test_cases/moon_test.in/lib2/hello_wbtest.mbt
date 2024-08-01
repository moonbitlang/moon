test {
  if hello() != "Hello, world!" {
    raise Failure("hello() != \"Hello, World\"")
  }
}
