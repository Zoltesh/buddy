.PHONY: dev build run clean telegram build-telegram

# Build the frontend and the Rust server
build: build-frontend build-server

build-frontend:
	cd frontend && npm install && npm run build

build-server:
	cargo build --release --bin buddy-server --bin buddy-telegram

build-telegram:
	cargo build --release --bin buddy-telegram

# Run the server (builds first)
run: build
	./target/release/buddy-server

# Dev mode: build frontend then run debug server
dev: build-frontend
	cargo run --bin buddy-server

# Run buddy-telegram in debug mode
telegram:
	cargo run --bin buddy-telegram

clean:
	cargo clean
	rm -rf frontend/dist frontend/node_modules
