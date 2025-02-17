pub fn hello_from_lib2() -> String {
  println("Hello from lib2")

  ""
}

test {
  println(@lib4.hello_from_lib4())
}