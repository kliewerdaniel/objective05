.PHONY: build test fmt clippy run

TOOLCHAIN_DIR := $(HOME)/.rustup/toolchains/1.91.0-aarch64-apple-darwin/bin
export PATH := $(TOOLCHAIN_DIR):$(PATH)

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
