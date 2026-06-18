#!/bin/bash
skripdir="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

cd "$skripdir" || exit $?
cargo build --release
