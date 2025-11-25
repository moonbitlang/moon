///|
fn moonbit_test_driver_internal_run_async_tests(
  _tests : Array[async () -> Unit],
) -> Unit {
  ignore(moonbit_test_driver_internal_max_concurrent_tests)
}

///|
fn moonbit_test_driver_internal_is_being_cancelled() -> Bool {
  false
}
