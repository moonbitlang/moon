pub fn hello_from_lib3() -> String {
  println("Hello from lib3")
  ""
}

test {
  println(hello_from_lib3())
}


test {
  println(@lib7.hello_from_lib7())
}