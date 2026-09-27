Bug: `add` returns the wrong sum.

`src/calc.sh` defines an `add` function that `tests/test_calc.sh` relies on. Calling `add 2 3`
should print `5`, but it currently prints `4`. Find the bug in `src/calc.sh` and fix it so
`tests/test_calc.sh` passes.
