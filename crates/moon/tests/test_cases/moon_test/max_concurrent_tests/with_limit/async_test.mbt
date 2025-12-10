async test "a1" {
  println("test 1 msg 1")
  @async.pause()
  println("test 1 msg 2")
}

async test "a2" {
  println("test 2 msg 1")
  @async.pause()
  println("test 2 msg 2")
}
