```mbt check
test {
  println(try? @strconv.parse_int("42"))
  let p = @immut/array.from_array([1, 2, 3])
  println(p)
}
```
