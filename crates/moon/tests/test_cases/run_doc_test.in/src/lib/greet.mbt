/// ```
/// println("doc_test 1 from greet.mbt")
/// ```
pub fn greet1() -> String {
  "greet, world!"
}


/// ```mbt
/// 
/// 
/// 
/// test {
///   println("test block 1")
/// }
/// 
/// 
/// test {
/// 
///   println("test block 2")
/// 
///   inspect!(1256)
/// 
/// 
/// 
/// }
/// 
/// test {
///   println("test block 3")
/// 
///   fail!("another failure")
/// }
/// 
/// ```
pub fn greet2() -> String {
  "greet, world!"
}

/// ```moonbit
/// 
/// println("doc_test 3 from greet.mbt")
/// 
/// 
/// 
/// ```
pub fn greet3() -> String {
  "greet, wor1ld!"
}

/// ```moonbit
/// test "panic test" { 
/// 
/// 
/// 
///   println("test block 4")
///   panic()
/// }
/// 
/// 
/// ```
pub fn greet() -> String {
  "greet, wor1ld!"
}


/// ```
/// test {
/// 
///   println("test block 5")
/// 
/// 
/// }
/// 
/// 
/// 
/// ```
pub fn greet4() -> String {
  "greet, wor1ld!"
}


/// ```
///   println("doc_test 5 from greet.mbt")
/// 
/// 
/// 
/// ```
pub fn greet5() -> String {
  "greet, wor1ld!"
}

/// ```
/// let buf = @buffer.new(size_hint=100)
/// buf.write_string("Tes")
/// buf.write_char('t')
/// inspect!(buf.contents(), content="")
/// inspect!(buf.contents())
/// 
///  let multi_line_string_1 = 
///     #|1
///     #|2
///     #|3
///     #|4
///     #|5
///     #|
/// 
///  inspect!(multi_line_string_1)
///  inspect!(multi_line_string_1, content="")
/// 
///  let multi_line_string_2 = 
///     #|1
///     #|2
///     #|3
/// 
///  inspect!(multi_line_string_2)
///  inspect!(multi_line_string_2, content="")
/// 
///  let multi_line_string_3 = 
///     #|1
///     #|2
/// 
///  inspect!(multi_line_string_3)
///  inspect!(multi_line_string_3, content="")
/// 
///  let multi_line_string_4 = 
///     #|1
/// 
///  inspect!(multi_line_string_4)
///  inspect!(multi_line_string_4, content="")
/// 
/// ```
pub fn greet6() -> String {
  "greet, wor1ld!"
}