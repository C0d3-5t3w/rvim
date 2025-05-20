.PHONY: build run test clean release install uninstall

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

# Install rvim to the system
install:
	@echo "Installing RVim..."
	@mkdir -p $(HOME)/.config/rvim
	@cp -r config/config.lua $(HOME)/.config/rvim/
	@echo "Config files installed to ~/.config/rvim/"
	@sudo cp target/debug/rvim /usr/local/bin/
	@echo "RVim binary installed to /usr/local/bin/"
	@echo "Installation complete!"

# Uninstall rvim from the system
uninstall:
	@echo "Uninstalling RVim..."
	@sudo rm /usr/local/bin/rvim
	@echo "RVim binary removed from /usr/local/bin/"
	@echo "Note: Configuration files in ~/.config/rvim/ were not removed."

dev:
	cargo watch -x run
