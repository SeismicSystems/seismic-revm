#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REVME_DIR="$SCRIPT_DIR/../bins/revme"
TESTS_DIR="$SCRIPT_DIR/../tests/seismic/bin"

cd "$REVME_DIR" || { echo "Failed to change directory to $REVME_DIR"; exit 1; }

cargo build --quiet || { echo "Build failed"; exit 1; }

run_test() {
    local filename=$(basename "$1")
    output=$("$REVME_DIR/../../target/debug/revme" evm --path "$1" 2>&1)
    if echo "$output" | grep -q "Result: Success"; then
        echo "PASS: $filename"
    else
        echo "FAIL: $filename"
        echo "  Error: $(echo "$output" | grep "Error:" | sed 's/^Error: //')"
        echo "  Details:"
        echo "$output" | sed 's/^/    /'
    fi
}

echo "Running tests..."
echo

for file in "$TESTS_DIR"/*; do
    [[ -f "$file" ]] && run_test "$file"
done

echo
echo "Tests completed."