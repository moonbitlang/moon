fn main {
  println("main func")
}


test {
  @lib.hello()
  println("------------------internal test in main pkg ------------------")
}