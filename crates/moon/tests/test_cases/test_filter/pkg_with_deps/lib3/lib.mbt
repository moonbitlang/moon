pub fn hello_from_lib3() -> String {
  println("Hello from lib3")
  println(@lib4.hello_from_lib4())
  ""
}

test {
  println(hello_from_lib3())
}