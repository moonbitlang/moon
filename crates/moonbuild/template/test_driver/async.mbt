type Moonbit_Test_Driver_Internal_Async_Context = @moonbitlang/async.TaskGroup[Unit]

fn moonbit_test_driver_internal_run_async_test(
  ctx : Moonbit_Test_Driver_Internal_Async_Context,
  name : String,
  f : async (Moonbit_Test_Driver_Internal_Test_Arg) -> Unit raise,
) -> Unit {
  let it = moonbit_test_driver_internal_new_test_arg(name)
  ctx.spawn_bg(() => f(it))
}

fn moonbit_test_driver_internal_run_async_main(
  f : (Moonbit_Test_Driver_Internal_Async_Context) -> Unit,
) -> Unit {
  @moonbitlang/async.run_async_main(fn() {
    @moonbitlang/async.with_task_group(group => f(group))
  })
}
