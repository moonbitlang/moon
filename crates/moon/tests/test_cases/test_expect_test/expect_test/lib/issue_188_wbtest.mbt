fn actual_188() -> String {
  #|State #0 (start)
  #|Transitions:
  #|  0.Assert(kind=BeginOfLine) -> State #1
  #|State #1
  #|

}

test "issue_188" {
  inspect(
    actual_188(),
    content=
      #|State #0
      #|Transitions:
      #|  0.Assert(kind=BeginOfLine) -> State #1
      #|State #1
      #|
    ,
  )
}
