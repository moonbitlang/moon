///|
fn main {
  println("Hello")
  f(None)
}

///|
fn f(x : Int?) -> Unit {
  g(x)
}

///|
fn g(x : Int?) -> Unit {
  let _ = x.unwrap()
}
