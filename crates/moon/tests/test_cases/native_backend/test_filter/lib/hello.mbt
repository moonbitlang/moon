pub fn hello() -> String {
  "Hello, world!"
}

test "A" {
  println("test A")
}

test "B" {
  println("test B")
}

test "inspect" {
  inspect(523)
  inspect("asdfhjas")
}

test "C" {
  println("test C")
}

test "D" (it: @test.Test) {
  it.writeln("test D")
  it.snapshot!(filename = "test.d")
}

