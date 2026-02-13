// This file is introduced when tests without args is needed but when async is needed
// This also introduces the dependency on async package

///|
let moonbit_test_driver_internal_async_tests : MoonBitTestDriverInternalTestMap[
  async () -> Unit,
] = {} // REPLACE ME: moonbit_test_driver_internal_async_tests

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_Async_No_Args with run_test(
  async_tests,
  filename,
  index,
  handle_result,
  error_to_string,
) {
  if moonbit_test_driver_internal_async_tests.0.get(filename) is Some(index_map) &&
    index_map.get(index) is Some((f, attrs)) {
    let name = if attrs is [name, ..] { name } else { "" }
    if MOONBIT_TEST_DRIVER_INTERNAL_IS_NATIVE {
      for attr in attrs {
        if attr is [.. "panic", ..] {
          raise MoonBitTestDriverInternalSkipTest(name~)
        }
      }
    }
    async_tests.push(() => {
      try f() catch {
        err if MoonBit_Async_Test_Driver_Impl::is_being_cancelled() => raise err
        err => handle_result(name, error_to_string(err), false)
      } noraise {
        _ => handle_result(name, "", false)
      }
    })
    true
  } else {
    false
  }
}
