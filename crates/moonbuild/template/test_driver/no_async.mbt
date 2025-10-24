type Moonbit_Test_Driver_Internal_Async_Context = Unit

fn moonbit_test_driver_internal_run_async_test(
  _ctx : Moonbit_Test_Driver_Internal_Async_Context,
  _name : String,
  _f : async (Moonbit_Test_Driver_Internal_Test_Arg) -> Unit raise,
  on_err~ : (Error) -> Unit,
) -> Unit {
  ignore(on_err)
  panic()
}

fn moonbit_test_driver_internal_run_async_main(
  f : (Moonbit_Test_Driver_Internal_Async_Context) -> Unit,
) -> Unit {
  f(())
}
