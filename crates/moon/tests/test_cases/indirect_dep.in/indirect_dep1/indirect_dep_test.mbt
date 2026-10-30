///|
test "indirect_dep_test" {
  inspect(@indirect_dep.f(), content="42")
}

///|
test "indirect_dep_test2" {
  inspect(@indirect_dep.g(), content="42")
}
