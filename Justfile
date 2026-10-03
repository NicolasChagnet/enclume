set default-list := true
set shell := ["bash", "-c"]

fmt:
    @echo "Running formatter..."
    @cargo fmt

lint:
    @echo "Running linter..."
    @cargo clippy

test:
    @echo "Running tests..."
    @cargo nextest run --test-threads="num-cpus" --no-tests=pass

ci: lint test

build profile:
    @cargo build --profile={{profile}}

run:
    @cargo run
