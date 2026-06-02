.PHONY: build test fmt clippy run

build:
	cargo build --workspace

test:
	cargo test --workspace

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

run:
	cargo run -p objective -- serve
