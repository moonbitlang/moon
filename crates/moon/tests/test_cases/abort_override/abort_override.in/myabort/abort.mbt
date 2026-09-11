pub fn[T] abort(msg : String) -> T {
  ignore(msg)
  println("---myabort---")
  panic()
}
