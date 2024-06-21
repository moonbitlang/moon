test {
  if hello() != "Hello, world!" {
    return Err("not ok")
  }
}
