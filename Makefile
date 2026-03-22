.PHONY: setup build run test lint fmt fmt-check clean \
        server-start server-stop server-setup server-logs

# First-time setup: ensure stable toolchain is up to date
setup:
	rustup update stable

# Compile a release binary
build:
	cargo build --release

# Run the app in debug mode
run:
	cargo run

# Run the app in release mode
run-release:
	cargo build --release && ./target/release/xmpp-start

# Run the full test suite (single-threaded — required for SQLite tests)
test:
	cargo test --bin xmpp-start -- --test-threads=1

# Run integration tests only
test-integration:
	cargo test --test critical_flows -- --test-threads=1

# Lint with clippy (warnings are errors)
lint:
	cargo clippy --bin xmpp-start -- -D warnings

# Auto-format all source files
fmt:
	cargo fmt

# Check formatting without modifying files (used in CI)
fmt-check:
	cargo fmt --check

# Remove build artifacts
clean:
	cargo clean

# ---- Local XMPP server (Prosody via Docker) ----

# Start the server in the background
server-start:
	docker compose up -d
	@echo "Waiting for Prosody to be ready..."
	@docker compose exec xmpp sh -c 'until prosodyctl status 2>/dev/null; do sleep 1; done' 2>/dev/null || sleep 4
	@echo "Server is up at localhost:5222"

# Stop and remove the container (data volume is kept)
server-stop:
	docker compose down

# Create test accounts (run once after server-start)
server-setup: server-start
	docker compose exec xmpp prosodyctl register alice localhost alice123
	docker compose exec xmpp prosodyctl register bob localhost bob123
	@echo ""
	@echo "Test accounts created:"
	@echo "  alice@localhost  password: alice123"
	@echo "  bob@localhost    password: bob123"
	@echo "  server: localhost  (leave server field blank or type localhost)"

# Tail server logs
server-logs:
	docker compose logs -f xmpp

# Nuke the data volume (fresh start)
server-reset:
	docker compose down -v
