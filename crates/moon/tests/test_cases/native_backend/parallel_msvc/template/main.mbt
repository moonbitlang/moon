///|
test {
  // Reference a lot of heavy stdlib components to enlarge the build,
  // so that MSVC may have more time to build stuff in parallel.
  //
  // This file will then be duplicated multiple times for the test.
  let r = @random.Rand::new()
  let bi = r.bigint(42)
  let tmp = bi + 100
  println("BigInt plus 100 is: " + tmp.to_string())
  let sm = @sorted_map.SortedMap::new()
  sm.set("key", 255.920)
  println(sm.get("key").unwrap())
  sm.set("key2", 3.14159)
  sm.remove("key")
  println(sm.to_string())
  let hm = @hashmap.HashMap::new()
  hm.set("foo", sm)
  println(hm.get("foo").unwrap())
}
