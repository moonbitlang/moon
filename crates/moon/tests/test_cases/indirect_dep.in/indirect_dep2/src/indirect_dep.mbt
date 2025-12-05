///|
pub fn f() -> Int {
  let t = @lib2.make_t()
  t.f()
}

///|
pub fn g() -> Int {
  let m = @sub.f()
  m.f()
}
