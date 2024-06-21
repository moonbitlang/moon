test {
  println("this is lib test")
  if hello() != "Hello, world!" {
    abort("")
  }
}
