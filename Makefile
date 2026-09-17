# A Homebrew rust ahead in PATH ignores rust-toolchain.toml and lacks the wasm32v1-none core.
export PATH := $(HOME)/.cargo/bin:$(PATH)

default: build

all: test

test: build
	cargo test

build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm

fmt:
	cargo fmt --all

clean:
	cargo clean
