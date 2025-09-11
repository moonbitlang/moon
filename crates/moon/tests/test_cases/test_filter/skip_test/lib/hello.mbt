///|
#skip("Testing skipping tests")
test "hello_0" {
  println("test hello_0")
}

///|
test "hello_1" {
  println("test hello_1")
}

///|
#skip("Skipping hello_2 test")
test "hello_2" {
  println("test hello_2")
}

///|
test "hello_3" {
  println("test hello_3")
}
