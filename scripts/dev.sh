#!/usr/bin/env bash
set -euo pipefail

cargo run -p objective -- setup
cargo run -p objective -- serve
