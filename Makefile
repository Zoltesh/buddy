.PHONY: dev build run clean

# Build the frontend and the Rust server
build: build-frontend build-server

build-frontend:
	cd frontend && npm install && npm run build

build-server:
	cargo build --release

# Run the server (builds first)
run: build
	./target/release/buddy-server

# Dev mode: build frontend then run debug server
dev: build-frontend
	cargo run --bin buddy-server

clean:
	cargo clean
	rm -rf frontend/dist frontend/node_modules
