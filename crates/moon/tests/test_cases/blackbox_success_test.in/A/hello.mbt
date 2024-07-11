pub fn hello() -> Unit {
  println("output from A/hello.mbt!")
}

fn _private_hello() -> Unit {
  println("output from A/hello.mbt::_private_hello")
}

struct StructA {
  a: Int
}

pub fn new() -> StructA {
  {
    a: 33
  }
}

pub fn method(self : StructA) -> Unit {
  println("self.a: " + self.a.to_string())
}

