pub fn abort[T](msg : String) -> T {
  ignore(msg)
  panic()
}
