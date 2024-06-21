pub fn hello_from_lib4() -> String {
  "Hello from lib4"
}

test {
  println(hello_from_lib4())
}