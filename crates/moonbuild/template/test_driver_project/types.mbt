// #region Define traits with default implementations

///|
trait MoonBit_Async_Test_Driver {
  run_async_tests(@moonbitlang/core/prelude.Array[async () -> Unit]) -> Unit = _
  is_being_cancelled() -> Bool = _
}

///|
let moonbit_test_driver_internal_max_concurrent_tests : Int = 10

///|
impl MoonBit_Async_Test_Driver with run_async_tests(_) {
  ignore(moonbit_test_driver_internal_max_concurrent_tests)
}

///|
impl MoonBit_Async_Test_Driver with is_being_cancelled() {
  false
}

///|
trait MoonBit_Test_Driver {
  /// returns if the test is found and executed
  run_test(@moonbitlang/core/prelude.Array[async () -> Unit], String, Int) -> Bool raise MoonBitTestDriverInternalSkipTest = _
}

///|
impl MoonBit_Test_Driver with run_test(_, _, _) {
  false
}

// #endregion
// #region Define abstract types for namespace with implementation declarations

///|
type MoonBit_Test_Driver_Internal_No_Args

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_No_Args

///|
type MoonBit_Test_Driver_Internal_With_Args

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_With_Args

///|
type MoonBit_Test_Driver_Internal_Async_No_Args

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_Async_No_Args

///|
type MoonBit_Test_Driver_Internal_Async_With_Args

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_Async_With_Args

///|
type MoonBit_Test_Driver_Internal_With_Bench_Args

///|
impl MoonBit_Test_Driver for MoonBit_Test_Driver_Internal_With_Bench_Args

///|
type MoonBit_Async_Test_Driver_Impl

///|
impl MoonBit_Async_Test_Driver for MoonBit_Async_Test_Driver_Impl

// #endregion

// #region Utility Types

///|
priv struct MoonBitTestDriverInternalTestMap[F](
  @moonbitlang/core/prelude.Map[
    String,
    @moonbitlang/core/prelude.Map[
      Int,
      (F, @moonbitlang/core/prelude.Array[String]),
    ],
  ]
)

///|
suberror MoonBitTestDriverInternalSkipTest {
  MoonBitTestDriverInternalSkipTest(name~ : String)
}

// #endregion
