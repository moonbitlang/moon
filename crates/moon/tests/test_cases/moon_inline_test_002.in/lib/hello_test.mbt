test {
  if hello() != "Hello, world!" {
    return Err("some error messages")
  }
}
