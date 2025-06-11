pub fn hello() -> String {
  "Hello, world!"
}

test "basic" {
  let buf = @buffer.new()
  let n = 123
  buf.write_string(n.to_string())
  inspect(buf)
}

test "indent" {
  let _ = {
    let buf = @buffer.new()
    let s = "haha"
    buf.write_string(s)
    inspect(buf)
  }
  ()
}

test "auto_reset" {
  let buf = @buffer.new()
  let s = "haha"
  buf.write_string(s)
  inspect(buf)
  let s = "123"
  buf.write_string(s)
  inspect(buf, content="123")
}

test "multiline-string" {
  let buf = @buffer.new()
  let s = "1\n2\n3\n"
  buf.write_string(s)
  inspect(buf)
}

test "multiline-string-2" {
  let buf = @buffer.new()
  let s =
    #|abc
    #|def
  buf.write_string(s)
  inspect(buf,
    content=
      #|bcd
      #|defg
    ,
  )
}

test "not-empty-expect" {
  let buf = @buffer.new()
  buf.write_string("123")
  inspect(buf, content="1")
}

test {
  inspect("a")
}

test {
  inspect("a")
}

test {
  inspect("a", content="")
}

test {
  inspect("a", content="b")
}

test {
  inspect("a\nb\nc\ndc,")
}

test {
  123 |> inspect()
}

test {
  123 |> inspect(content="")
}

test {
  "a\nb\nc\n" |> inspect()
}

test {
  "a\nb\nc\n" |> inspect(content="")
}

test {
  "a\nb\nc\n"
  |> inspect(
    content=
      #|1
      #|2
      #|3
    ,
  )
}

test {
  "a\nb\nc\n" |> inspect(content="") |> inspect(content="")
}

test {
  inspect("ha", content="哈哈")
}

test {
  inspect("haha", content="哈哈")
}

test {
  inspect("hahaha", content="哈哈")
}

test {
  inspect("哈哈", content="haha")
}

test {
  inspect("哈哈哈", content="haha")
}

test {
  let buf = @buffer.new()
  buf.write_string("just\ntest")
  inspect(buf)
}

test "hello" {
  let buf = @buffer.new()
  buf.write_string("just\ntest")
  inspect(buf, content="")
}

fn actual() -> String {
  "BinOp('+', BinOp('+', Num(1), Num(2)), Num(3))"
}

test {
  inspect(actual(), content="BinOp('+', Num(1), Num(2))")
}

test {
  let xs = Array::from_fixed_array(["a", "b", "c"])
  inspect(xs)
}
