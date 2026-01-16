type Moonbit_Test_Driver_Internal_Bench_Arg = @moonbitlang/core/bench.T

fn moonbit_test_driver_internal_new_bench_arg() -> Moonbit_Test_Driver_Internal_Bench_Arg {
  @moonbitlang/core/bench.new()
}

fn moonbit_test_driver_internal_get_bench_summary(
  bench : Moonbit_Test_Driver_Internal_Bench_Arg,
) -> String {
  let s = bench.dump_summaries()
  "@BATCH_BENCH { \"summaries\": \{s} }"
}
