set default-list := true
set shell := ["bash", "-c"]

fmt:
    @echo "Running formatter..."
    @cargo fmt --all

lint:
    @echo "Running linter..."
    @cargo fmt --all --check
    @cargo clippy --workspace -- -D warnings

test:
    @echo "Running tests..."
    @cargo nextest run --workspace --test-threads="num-cpus" --no-tests=pass

ci: lint test

build profile:
    @cargo build --profile={{profile}}

run:
    @cargo run
