type Moonbit_Test_Driver_Internal_Bench_Arg = Unit

fn moonbit_test_driver_internal_new_bench_arg() -> Moonbit_Test_Driver_Internal_Bench_Arg {
  ()
}

fn moonbit_test_driver_internal_get_bench_summary(
  bench : Moonbit_Test_Driver_Internal_Bench_Arg,
) -> String {
  ignore(bench)
  ""
}
