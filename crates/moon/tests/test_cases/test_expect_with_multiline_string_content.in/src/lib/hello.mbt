///|
test {
  inspect("\"abc\"")
  inspect("\"abc\"", 
    content=#|"err"
  )
  inspect("\"abc\"", content=(#|"err"
  ))
  inspect(
    "\"a\nb\nc\"",
    content=(
      #|"err
      #|b
      #|c"
    ),
  )
}
