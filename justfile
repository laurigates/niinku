# niinku — Finnish puhekieli/slang dictionary for HeliBoard.
# Run `just` or `just --list` to see all recipes.

set positional-arguments

default:
    @just --list

##########
# Build
##########

# Build all crates in release mode.
[group: "build"]
build:
    cargo build --release --workspace

##########
# Quality
##########

# Run unit and integration tests.
[group: "quality"]
test:
    cargo test --workspace

# Run clippy with -D warnings and check formatting.
[group: "quality"]
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Auto-fix formatting.
[group: "quality"]
fmt:
    cargo fmt --all

##########
# Pipeline
##########

# Stage A: ingest a single source into data/cached/.
[group: "pipeline"]
ingest *args:
    cargo run --release -p niinku-pipeline -- ingest "$@"

# Stage B: assemble cached + live sources into a `.combined` wordlist.
[group: "pipeline"]
assemble *args:
    cargo run --release -p niinku-pipeline -- assemble "$@"
