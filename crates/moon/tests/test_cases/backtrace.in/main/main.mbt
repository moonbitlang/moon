fn foo() -> Unit {
  abort("hah")
}

fn bar() -> Unit {
  foo()
}

fn main {
  bar()
}
