test {
  if hello() != "Hello, world!" {
    return Err("hello != world")
  }
}
