// This file is introduced when tests with test arg is needed
// This also introduces the dependency on @moonbitlang/core/test package

///|
let moonbit_test_driver_internal_with_args_tests : MoonBitTestDriverInternalTestMap[
  (@moonbitlang/core/test.Test) -> Unit raise,
] = {} // REPLACE ME: moonbit_test_driver_internal_with_args_tests

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_With_Args with run_test(
  _,
  filename,
  index,
  handle_result,
  error_to_string,
) {
  if moonbit_test_driver_internal_with_args_tests.0.get(filename)
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
    moonbit_test_driver_internal_catch_error(
      () => f(@moonbitlang/core/test.new(name)),
      on_ok=() => handle_result(name, "", false),
      on_err=err => handle_result(name, error_to_string(err), false),
    )
    true
  } else {
    false
  }
}
