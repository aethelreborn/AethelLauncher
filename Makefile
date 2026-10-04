
.PHONY: run build backend test check fmt clippy clean ipc-client ipc-e2e install-linux

run:
	cargo run --release -p launcher-bin

build:
	cargo build --release -p launcher-bin

backend:
	cargo run --release -p backend

test:
	cargo test --workspace

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

ipc-client:
	javac -d gamesupport/ipc-client/out $$(find gamesupport/ipc-client -name '*.java')

ipc-e2e: ipc-client
	cargo test -p launcher-core java_smoke -- --ignored --nocapture

install-linux: build
	install -Dm755 target/release/aethel-launcher $(HOME)/.local/bin/aethel-launcher
	install -Dm644 packaging/linux/aethel-launcher.desktop $(HOME)/.local/share/applications/aethel-launcher.desktop

clean:
	cargo clean
	rm -rf gamesupport/ipc-client/out
