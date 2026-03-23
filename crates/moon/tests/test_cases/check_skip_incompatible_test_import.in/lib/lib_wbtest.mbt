test "whitebox uses js-only dep" {
  ignore(@wbonly.dep())
  ignore(value())
}
