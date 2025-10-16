typealias @moonbitlang/async.TaskGroup[Unit]
  as Moonbit_Test_Driver_Internal_Async_Context 

fn moonbit_test_driver_internal_run_async_test(
  ctx : Moonbit_Test_Driver_Internal_Async_Context,
  name : String,
  f : async (Moonbit_Test_Driver_Internal_Test_Arg) -> Unit raise,
  on_err~ : (Error) -> Unit
) -> Unit {
  let it = moonbit_test_driver_internal_new_test_arg(name)
  try {
    // Currently, `spawn_bg` may raise error in `moonbitlang/async`.
    // However, this error is intended to be removed.
    // So we need to make sure the code here is compatible with
    // both the old version and new version.
    if true {
      ctx.spawn_bg(() => f(it))
    } else {
      raise Failure("")
    }
  } catch { err => on_err(err) }
}

fn moonbit_test_driver_internal_run_async_main(
  f : (Moonbit_Test_Driver_Internal_Async_Context) -> Unit,
) -> Unit {
  @moonbitlang/async.run_async_main(fn() {
    @moonbitlang/async.with_task_group(group => f(group))
  })
}
