test "hello" {
  println("hello from hello_test.mbt")
}

fn test_hello() -> String {
  "hello from hello_test.mbt"
}


test "inspect in bbtest" {
  fn_in_md_test()
  inspect!("inspect in bbtest")
  let a = #| all
        #| wishes
        #|
        #| come
        #| true
        #|
  inspect!(a)
}

