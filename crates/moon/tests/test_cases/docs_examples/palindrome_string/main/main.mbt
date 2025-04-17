fn main {
  let test1 = "abc"
  let test2 = "aba"
  let a = @lib.is_palindrome(test1)
  let b = @lib.is_palindrome(test2)
  if a {
    println(test1) // not print
  }
  if b {
    println(test2) // print "aba"
  }
}
