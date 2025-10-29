pub fn hello() -> String {
  "Hello, world!"
}

test "test inspect 1" {
  inspect("a")
  inspect("b")
}

test "test snapshot 1" (it : @test.Test) {
  it.writeln("hello")
  it.writeln("snapshot")
  it.writeln("testing")
  it.snapshot!(filename="001.txt")
}

test "test inspect 2" {
  inspect("c")
  inspect("d")
}

test "test snapshot 2" (it : @test.Test) {
  it.writeln("should")
  it.writeln("be")
  it.writeln("work")
  it.snapshot!(filename="002.txt")
}
