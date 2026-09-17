///|
type Token

///|
struct Node {
  token : Token
  mut next : Node?
}

///|
extern "C" fn token_new() -> Token = "cycle_test_token_new"

///|
extern "C" fn finalized_count() -> Int = "cycle_test_finalized_count"

///|
extern "C" fn collect_cycles() -> Unit = "moonbit_collect_cycles_for_testing"

///|
#inline(never)
fn make_cycle() -> Unit {
  let node = Node::{ token: token_new(), next: None }
  node.next = Some(node)
}

///|
fn main {
  make_cycle()
  println("before collection: \{finalized_count()}")
  collect_cycles()
  println("after collection: \{finalized_count()}")
}
