install:
    cargo install --path ./crates/moon --debug --offline --root ~/.moon --force --locked

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

add-header:
    hawkeye format

install-moonrun:
    cargo install --path ./crates/moonrun --debug --offline --root ~/.moon --force --locked
