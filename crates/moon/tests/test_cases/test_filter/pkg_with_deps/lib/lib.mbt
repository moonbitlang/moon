test "hello_lib" {
  println(@lib1.hello_from_lib1())
}

pub fn hello() -> String {
  "hello from lib"
}