///|
#cfg(target="native")
#test_entry
fn main {
  let async_tests = []
  for arg in moonbit_test_driver_internal_native_parse_args() {
    moonbit_test_driver_internal_do_execute(async_tests, arg.0, arg.1)
  }
  MoonBit_Async_Test_Driver_Impl::run_async_tests(async_tests)
}

///|
#cfg(target="js")
#test_entry
fn main {
  let test_params = moonbit_test_driver_js_parse_args()
  moonbit_test_driver_internal_execute(test_params)
  moonbit_test_driver_finish()
}

///|
#cfg(any(target="wasm", target="wasm-gc"))
#test_entry
fn main {
  ()
}

///|
#cfg(target="js")
extern "js" fn moonbit_test_driver_js_parse_args() -> Array[
  MoonBitTestDriverInternalTestEntry,
] =
  #|() => {
  #|  try {
  #|    let testParams = []
  #|    // check if there is command line argument
  #|    if (process.argv.length > 2) {
  #|      const testArgs = JSON.parse(process.argv[2])
  #|      testParams = testArgs.file_and_index.flatMap(([file, ranges]) => {
  #|        let arr = []
  #|        for (let range of ranges) {
  #|          for (let i = range.start; i < range.end; i++) {
  #|            arr.push([file, i])
  #|          }
  #|        }
  #|        return arr
  #|      })
  #|      return testParams
  #|    }
  #|  } catch (error) {
  #|    console.error('failed to parse args: ', error.message)
  #|    process.exit(1)
  #|  }
  #|}

///|
#cfg(target="js")
pub fn moonbit_test_driver_internal_execute(
  tests : Array[MoonBitTestDriverInternalTestEntry],
) -> Unit {
  let async_tests = []
  for i in 0..<tests.length() {
    let entry = tests[i]
    moonbit_test_driver_internal_do_execute(
      async_tests,
      entry.filename(),
      entry.index(),
    )
  }
  MoonBit_Async_Test_Driver_Impl::run_async_tests(async_tests)
}

///|
#cfg(any(target="wasm", target="wasm-gc"))
pub fn moonbit_test_driver_internal_execute(
  filename : MoonbitTestDriverInternalExternString,
  index : Int,
) -> Unit {
  let filename = moonbit_test_driver_internal_get_file_name(filename)
  let async_tests = []
  moonbit_test_driver_internal_do_execute(async_tests, filename, index)
  MoonBit_Async_Test_Driver_Impl::run_async_tests(async_tests)
}

///|
#cfg(not(target="native"))
pub fn moonbit_test_driver_finish() -> Unit {
  // {COVERAGE_END}
}

///|
fn moonbit_test_driver_internal_do_execute(
  async_tests : Array[async () -> Unit],
  filename : String,
  index : Int,
) -> Unit {
  // These two are internalized to avoid unused warnings

  fn handle_result(testname : String, message : String, skipped : Bool) {
    // In WASM, this function is called by the external test driver, but here
    // in native mode there's nothing that calls it, so we call it manually.
    if not(skipped) && MOONBIT_TEST_DRIVER_INTERNAL_IS_NATIVE {
      // {COVERAGE_END}
    }
    let file_name = filename.escape()
    let test_name = testname.escape()
    let message = message.escape()
    @moonbitlang/core/prelude.println("{BEGIN_MOONTEST}")
    @moonbitlang/core/prelude.println(
      "{\"package\": \"{PACKAGE}\", \"filename\": \{file_name}, \"index\": \"\{index}\", \"test_name\": \{test_name}, \"message\": \{message}}",
    )
    @moonbitlang/core/prelude.println("{END_MOONTEST}")
  }

  fn error_to_string(err : Error) -> String {
    match err {
      @moonbitlang/core/prelude.Failure::Failure(e)
      | @moonbitlang/core/prelude.InspectError::InspectError(e)
      | @moonbitlang/core/prelude.SnapshotError::SnapshotError(e)
      | MoonBitTestDriverInternalJsError(e) => e
      e => moonbit_test_driver_internal_error_to_string(e)
    }
  }

  try {
    if !(MoonBit_Test_Driver_Internal_No_Args::run_test(
        async_tests, filename, index, handle_result, error_to_string,
      ) ||
      MoonBit_Test_Driver_Internal_With_Args::run_test(
        async_tests, filename, index, handle_result, error_to_string,
      ) ||
      MoonBit_Test_Driver_Internal_Async_No_Args::run_test(
        async_tests, filename, index, handle_result, error_to_string,
      ) ||
      MoonBit_Test_Driver_Internal_Async_With_Args::run_test(
        async_tests, filename, index, handle_result, error_to_string,
      ) ||
      MoonBit_Test_Driver_Internal_With_Bench_Args::run_test(
        async_tests, filename, index, handle_result, error_to_string,
      )) {
      raise MoonBitTestDriverInternalSkipTest(name="")
    }
  } catch {
    err => {
      guard err is MoonBitTestDriverInternalSkipTest(name~)
      handle_result(name, "skipped test", true)
    }
  }
}
