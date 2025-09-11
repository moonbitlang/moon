pub let hello_list : @list.List[String] = @list.of(
  ["Hello, ", "world!"],
)

pub fn hello() -> String {
  hello_list.to_array().join("")
}
