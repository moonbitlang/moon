// This file is introduced when tests without args is needed

///|
let moonbit_test_driver_internal_no_args_tests : MoonBitTestDriverInternalTestMap[
  () -> Unit raise,
] = {} // REPLACE ME: moonbit_test_driver_internal_no_args_tests

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_No_Args with run_test(
  _,
  filename,
  index,
) {
  if moonbit_test_driver_internal_no_args_tests.0.get(filename)
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
      f,
      on_ok=() => moonbit_test_driver_internal_handle_result(filename, index, name, "", false),
      on_err=err => moonbit_test_driver_internal_handle_result(
        filename,
        index,
        name,
        moonbit_test_driver_internal_error_to_string(err),
        false,
      ),
    )
    true
  } else {
    false
  }
}
