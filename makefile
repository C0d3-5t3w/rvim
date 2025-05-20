.PHONY: build run test clean release install

build:
	cargo build

run:
	cargo run

test:
	cargo test

clean:
	cargo clean

release:
	cargo build --release

install:
	@sudo cp target/debug/rvim /usr/local/bin/

dev:
	cargo watch -x run
