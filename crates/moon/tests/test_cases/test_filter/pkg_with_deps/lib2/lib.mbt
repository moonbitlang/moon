pub fn hello_from_lib2() -> String {
  println("Hello from lib2")
  println(@lib4.hello_from_lib4())
  ""
}

test {
  println(hello_from_lib2())
}