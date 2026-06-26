// This file is included if async test is needed
// This also introduces the dependency on async package

///|
#cfg(not(target="wasm-gc"))
impl MoonBit_Async_Test_Driver for MoonBit_Async_Test_Driver_Impl with fn run_async_tests(
  tests,
) {
  let moonbit_test_driver_internal_max_concurrent_tests : Int = 0 // REPLACE ME: moonbit_test_driver_internal_max_concurrent_tests
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
    // {COVERAGE_END}
  })
}

///|
#cfg(not(target="wasm-gc"))
impl MoonBit_Async_Test_Driver for MoonBit_Async_Test_Driver_Impl with fn is_being_cancelled() {
  @moonbitlang/async.is_being_cancelled()
}
