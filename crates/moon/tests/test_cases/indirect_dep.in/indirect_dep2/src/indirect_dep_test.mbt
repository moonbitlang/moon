///|
test "indirect_dep_test" {
  inspect(f(), content="42")
}

///|
test "indirect_dep_test2" {
  inspect(g(), content="42")
}
