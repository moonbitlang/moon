using @dep { type Foo }

fn helper() -> Int {
  1
}

pub fn value() -> Int {
  match Some(1) {
    Some(x) => 1
    None => 0
  }
}
