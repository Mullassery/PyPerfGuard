# Exit codes

PyPerfGuard uses stable exit codes so CI systems can rely on them. They are
asserted by integration tests.

| Code | Name | Meaning |
| ---: | --- | --- |
| `0` | success | Everything passed, or only warnings/info/inconclusive-without-blocking |
| `1` | regression | A blocking regression policy was violated |
| `2` | config | Invalid configuration, malformed or incompatible input, usage error |
| `3` | execution | A benchmark failed to run, exited non-zero, or timed out |
| `4` | inconclusive | A comparison was inconclusive and `fail_on_inconclusive` is set |
| `5` | internal | An unexpected internal error |

## Outcome precedence

When several rules apply, the most severe outcome wins:

```text
error > regression > warning > inconclusive > info > pass
```

Exit codes map from the combined outcome:

- `pass`, `info`, `warning` → `0`
- `regression` → `1`
- `inconclusive` → `4` if `fail_on_inconclusive`, otherwise `0`
- `error` → `3`

## Command notes

- `run` exits `3` when any benchmark run completely fails; otherwise `0`.
  `run` is not a gate — use `check` for policy enforcement.
- `check` exits according to the table above.
- `compare`, `report`, `history`, `baseline`, `doctor`, `version` exit `0` on success.
- `validate` exits `2` when any input is invalid.
