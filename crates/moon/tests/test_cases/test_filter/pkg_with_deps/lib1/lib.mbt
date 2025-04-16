pub fn hello_from_lib1() -> String {
  println("Hello from lib1")
  println(@lib2.hello_from_lib2())
  println(@lib3.hello_from_lib3())
  ""
}

test {
  println(hello_from_lib1())
}
