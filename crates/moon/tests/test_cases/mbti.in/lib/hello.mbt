pub let hello_list : @immut/list.T[String] = @immut/list.of(
  ["Hello, ", "world!"],
)

pub fn hello() -> String {
  String::concat(separator="", hello_list.to_array())
}
