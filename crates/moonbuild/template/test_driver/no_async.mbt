///|
fn moonbit_test_driver_internal_run_async_tests(
  _tests : @moonbitlang/core/prelude.Array[async () -> Unit],
) -> Unit {
  @moonbitlang/core/prelude.ignore(moonbit_test_driver_internal_max_concurrent_tests)
}

///|
fn moonbit_test_driver_internal_is_being_cancelled() -> Bool {
  false
}
