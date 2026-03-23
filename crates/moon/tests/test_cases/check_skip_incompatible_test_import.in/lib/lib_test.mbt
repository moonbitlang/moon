test "blackbox uses js-only dep" {
  ignore(@testonly.dep())
  ignore(@lib.value())
}
