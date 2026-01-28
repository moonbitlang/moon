use super::*;

#[test]
fn test_query_not_in_project() {
    let dir = TestDir::new("query_symbol.in/empty");

    let output = get_stdout(&dir, ["doc", "String::from_array"]);
    let output = output
        .lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    check(
        output,
        expect![[r#"
            package "moonbitlang/core/string"
            pub fn String::from_array(ArrayView[Char]) -> String
              Convert char array to string.
               ```mbt check
               test {
                 let s = String::from_array(['H', 'e', 'l', 'l', 'o'])
                 assert_eq(s, "Hello")
               }
               ```
               Do not convert large data to `Array[Char]` and build a string with `String::from_array`.
               For efficiency considerations, it's recommended to use `Buffer` instead."#]],
    );

    // nightly moonc has different output for this case
    // let output = get_stdout(&dir, ["doc", "String::fromxxx"]);
    // check(
    //     output,
    //     expect![[r#"
    //     package "moonbitlang/core/string"

    //     no child symbol match query `fromxxx`
    // "#]],
    // );
}

#[test]
fn test_query_in_project() {
    let dir = TestDir::new("query_symbol.in/proj");

    let output = get_stdout(&dir, ["doc", "fib"]);
    check(
        output,
        expect![[r#"
            package "username/proj"
            pub fn fib(Int) -> Int64
        "#]],
    );
}
