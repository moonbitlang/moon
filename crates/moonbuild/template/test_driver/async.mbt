///|
fn moonbit_test_driver_internal_run_async_tests(
  tests : @moonbitlang/core/prelude.Array[async () -> Unit],
) -> Unit {
  @moonbitlang/async.run_async_main(() => {
    @moonbitlang/async.with_task_group(group => {
      let mut i = 0
      for _ in 0..<moonbit_test_driver_internal_max_concurrent_tests {
        group.spawn_bg(() => {
          while i < tests.length() {
            let f = tests[i]
            i += 1
            f()
          }
        })
      }
    })
  })
}

///|
fn moonbit_test_driver_internal_is_being_cancelled() -> Bool {
  @moonbitlang/async.is_being_cancelled()
}
