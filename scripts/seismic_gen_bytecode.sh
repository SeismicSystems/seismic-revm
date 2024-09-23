#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS_DIR="$SCRIPT_DIR/../tests/seismic"

echo "Generating bytecode of .sol test files..."

for file in "$TESTS_DIR/sol"/*.sol; do
    if [ -f "$file" ]; then
        solc --bin "$file" -o "$TESTS_DIR/bin" --overwrite
    fi
done

echo "Bytecode generation complete."
