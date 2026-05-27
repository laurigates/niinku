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

# Run every check that CI runs (fmt + clippy + test).
[group: "quality"]
ci: lint test

##########
# Pipeline
##########

# Stage A: ingest a single source into data/cached/.
[group: "pipeline"]
ingest *args:
    cargo run --release -p niinku-cli -- ingest "$@"

# Stage B: assemble cached + live sources into a `.combined` wordlist.
[group: "pipeline"]
assemble *args:
    cargo run --release -p niinku-cli -- assemble "$@"

# Download a Finnish OpenSubtitles frequency list (Hermit Dave 2018,
# top 50k tokens) into data/cached/. Pass FULL=1 for the much larger
# fi_full.txt instead.
[group: "pipeline"]
download:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p data/cached
    out=data/cached/opensubtitles-fi.txt
    if [[ -s "$out" ]]; then
      echo "$out already present ($(wc -l < "$out") lines); delete to re-fetch"
      exit 0
    fi
    file="${FULL:+fi_full.txt}"
    file="${file:-fi_50k.txt}"
    url="https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/fi/$file"
    echo "fetching $url -> $out"
    curl -fsSL "$url" -o "$out"
    echo "wrote $(wc -l < "$out") lines"

# Download dicttool_aosp.jar from remi0s/aosp-dictionary-tools.
[group: "pipeline"]
download-jar:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p tools
    out=tools/dicttool_aosp.jar
    if [[ -s "$out" ]]; then
      echo "$out already present"
      exit 0
    fi
    url=https://raw.githubusercontent.com/remi0s/aosp-dictionary-tools/master/dicttool_aosp.jar
    echo "fetching $url -> $out"
    curl -fsSL "$url" -o "$out"

# Compile data/out/niinku.combined → data/out/puhekieli_fi.dict via dicttool.
[group: "pipeline"]
compile: download-jar
    cargo run --release -p niinku-cli -- compile \
        --combined data/out/niinku.combined \
        --output   data/out/puhekieli_fi.dict

# End-to-end: download corpus + jar, assemble .combined, compile to .dict.
[group: "pipeline"]
generate: download download-jar
    mkdir -p data/out
    cargo run --release -p niinku-cli -- assemble --output data/out/niinku.combined
    cargo run --release -p niinku-cli -- compile \
        --combined data/out/niinku.combined \
        --output   data/out/puhekieli_fi.dict
