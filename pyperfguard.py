# Minimal Python shim for PyPI distribution via maturin (bin bindings)
from __future__ import annotations

import logging
import os
import sys
from pathlib import Path
import subprocess

logger = logging.getLogger(__name__)

# Marks a child process launched by this shim. If the "binary" we locate turns
# out to be this same console-script shim, the child sees this and exits
# instead of spawning itself forever (infinite recursion).
_REENTRY_ENV = "PYPERFGUARD_SHIM_REENTRY"


def _binary_candidates() -> list[Path]:
    base = Path(sys.executable).parent / "pyperfguard"
    # On Windows maturin installs a ".exe"; on POSIX the bare name is correct.
    return [base, base.with_suffix(".exe")]


def main() -> int:
    if os.environ.get(_REENTRY_ENV) == "1":
        print(
            "pyperfguard: refusing to re-invoke the Python shim recursively; "
            "the Rust binary could not be located.",
            file=sys.stderr,
        )
        return 2

    # Locate the bundled Rust binary installed by maturin.
    for candidate in _binary_candidates():
        if candidate.exists() and candidate.is_file():
            env = os.environ.copy()
            env[_REENTRY_ENV] = "1"
            try:
                proc = subprocess.run([str(candidate)] + sys.argv[1:], env=env)
                return proc.returncode
            except OSError as exc:
                logger.warning(
                    "failed to run bundled pyperfguard binary %s: %s", candidate, exc
                )

    # Fallback: build/run via cargo for development checkouts. Never fall back
    # to a bare "pyperfguard" on PATH, since that can resolve back to this shim.
    repo_root = Path(__file__).resolve().parent
    cargo = [
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        str(repo_root / "Cargo.toml"),
        "--",
    ]
    try:
        proc = subprocess.run(cargo + sys.argv[1:])
        return proc.returncode
    except FileNotFoundError:
        logger.warning("cargo not found; cannot build pyperfguard from source")
    except OSError as exc:
        logger.warning("failed to run pyperfguard via cargo: %s", exc)

    print("pyperfguard binary not found; install the Rust binary or rebuild", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
