# Default: show available commands
default:
    @just --list

# Build the CLI
build mode="dev":
    @if [ "{{mode}}" = "release" ]; then \
        cargo build --release; \
    else \
        cargo build; \
    fi

# Run the CLI
run *args:
    cargo run -- {{args}}

# Run tests
test *args:
    cargo test {{args}}

# Format source
fmt:
    cargo fmt

# Lint source
lint:
    cargo clippy -- -D warnings

# Remove build artifacts
clean:
    cargo clean

# Full reset
reset: clean build
