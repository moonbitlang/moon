///|
async test "timer sleep resumes" {
  @async.sleep(1)
  println("timer resumed")
}
