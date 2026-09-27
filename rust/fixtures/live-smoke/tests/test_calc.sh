#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
source src/calc.sh

result="$(add 2 3)"
if [ "$result" != "5" ]; then
  echo "FAIL: add(2, 3) = $result, expected 5"
  exit 1
fi
echo "PASS: add(2, 3) = 5"
