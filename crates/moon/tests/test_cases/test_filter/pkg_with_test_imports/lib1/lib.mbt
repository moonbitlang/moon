pub fn hello_from_lib1() -> String {
  println("Hello from lib1")

  ""
}

test {
  println(@lib3.hello_from_lib3())
}
