
dependencies:
	echo "Installing nightly toolchain if not already installed..." && \
	rustup toolchain install nightly --allow-downgrade && \
	echo "Installing wasm32 target..." && \
	rustup target add wasm32-unknown-unknown && \
	echo "Installing trunk..." && \
	cargo install trunk --locked && \
	cargo install --locked wasm-bindgen-cli

build:
	echo "Building frontend and backend..." && \
	cd frontend && env NO_COLOR=true trunk build --release && cd .. && \
	echo "Frontend build complete." && \
	cargo build --release && \
	echo "Backend build complete."

install:
	$(MAKE) dependencies
	$(MAKE) build
	sudo cp target/release/wakezilla /usr/local/bin/wakezilla && \
	echo "Installation complete."

release:
	$(MAKE) build
	cargo publish

docker-build:
	docker build -t wakezilla .


# Mac/Windows with Docker Desktop: WOL broadcast won't reach LAN (VM isolation)
# This mode is for testing the web interface only
# For full WOL support on Mac, run natively: cargo run -- proxy-server
# Linux: uses --network host for full WOL broadcast support
docker-run:
	docker run --rm --network host -v ${PWD}/wakezilla-data:/opt/wakezilla wakezilla proxy-server
