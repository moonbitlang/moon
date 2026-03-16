test "binary_search smoke test" {
  if @invpred.binary_search([1, 3, 5, 7], 5) != 2 {
    fail("binary_search([1, 3, 5, 7], 5) != 2")
  }
}
