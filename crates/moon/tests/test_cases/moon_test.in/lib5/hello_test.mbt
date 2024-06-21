test {
  return Ok(())
}

test "" {

}

test "without_space" {
  return Ok(())
}

test "with space" {
  return Ok(())
}

// TODO:
// the output of debug("with space and \" and \\") is not correct

// test "with space and \"" {
//   return Ok(())
// }
// test "with space and \" and \\" {
//   return Ok(())
// }
// test "with space and \" and \\ and one line" {
//   let _ = "\" \" \\"
//   return Ok(())
// }
