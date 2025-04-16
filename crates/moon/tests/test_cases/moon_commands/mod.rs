use super::*;

#[test]
fn test_moon_cmd() {
    let dir = TestDir::new("moon_commands");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/list/lib.mbt -o ./target/wasm-gc/release/build/lib/list/list.core -pkg design/lib/list -pkg-sources design/lib/list:./lib/list -target wasm-gc
            moonc build-package ./lib/queue/lib.mbt -o ./target/wasm-gc/release/build/lib/queue/queue.core -pkg design/lib/queue -i ./target/wasm-gc/release/build/lib/list/list.mi:list -pkg-sources design/lib/queue:./lib/queue -target wasm-gc
            moonc build-package ./main2/main.mbt -o ./target/wasm-gc/release/build/main2/main2.core -pkg design/main2 -is-main -i ./target/wasm-gc/release/build/lib/queue/queue.mi:queue -pkg-sources design/main2:./main2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/list/list.core ./target/wasm-gc/release/build/lib/queue/queue.core ./target/wasm-gc/release/build/main2/main2.core -main design/main2 -o ./target/wasm-gc/release/build/main2/main2.wasm -pkg-config-path ./main2/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main2:./main2 -target wasm-gc
            moonc build-package ./main1/main.mbt -o ./target/wasm-gc/release/build/main1/main1.core -pkg design/main1 -is-main -i ./target/wasm-gc/release/build/lib/queue/queue.mi:queue -pkg-sources design/main1:./main1 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/list/list.core ./target/wasm-gc/release/build/lib/queue/queue.core ./target/wasm-gc/release/build/main1/main1.core -main design/main1 -o ./target/wasm-gc/release/build/main1/main1.wasm -pkg-config-path ./main1/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main1:./main1 -target wasm-gc
        "#]],
    );
}
