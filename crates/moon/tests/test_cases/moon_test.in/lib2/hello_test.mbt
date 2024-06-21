test {
  if hello() != "Hello, world!" {
    return Err("hello() != \"Hello, World\"")
  }
}
