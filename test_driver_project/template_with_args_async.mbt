// This file is introduced when tests with test arg is needed and async is needed
// This also introduces the dependency on @moonbitlang/core/test and @moonbitlang/async package

///|
let moonbit_test_driver_internal_async_tests_with_args : MoonBitTestDriverInternalTestMap[
  async (@moonbitlang/core/test.Test) -> Unit,
] = {} // REPLACE ME: moonbit_test_driver_internal_async_tests_with_args

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_Async_With_Args with run_test(
  async_tests,
  filename,
  index,
) {
  if moonbit_test_driver_internal_async_tests_with_args.0.get(filename)
    is Some(index_map) &&
    index_map.get(index) is Some((f, attrs)) {
    let name = if attrs is [name, ..] { name } else { "" }
    if MOONBIT_TEST_DRIVER_INTERNAL_IS_NATIVE {
      for attr in attrs {
        if attr is [.. "panic", ..] {
          raise MoonBitTestDriverInternalSkipTest(name~)
        }
      }
    }
    async_tests.push(() => try f(@moonbitlang/core/test.new(name)) catch {
      err if MoonBit_Async_Test_Driver_Impl::is_being_cancelled() => raise err
      err =>
        moonbit_test_driver_internal_handle_result(
          filename,
          index,
          name,
          moonbit_test_driver_internal_error_to_string(err),
          false,
        )
    } noraise {
      _ =>
        moonbit_test_driver_internal_handle_result(
          filename, index, name, "", false,
        )
    })
    true
  } else {
    false
  }
}
