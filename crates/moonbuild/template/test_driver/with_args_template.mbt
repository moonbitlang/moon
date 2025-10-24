type Moonbit_Test_Driver_Internal_Test_Arg = @moonbitlang/core/test.T

fn moonbit_test_driver_internal_new_test_arg(
  name : String,
) -> Moonbit_Test_Driver_Internal_Test_Arg {
  @moonbitlang/core/test.new(name)
}
