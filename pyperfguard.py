# Minimal Python shim for PyPI distribution via maturin (bin bindings)
from __future__ import annotations

import sys
from pathlib import Path
import subprocess


def main() -> int:
    # Locate the bundled Rust binary installed by maturin
    try:
        binary = Path(sys.executable).parent / "pyperfguard"
        if not binary.exists():
            # On Windows, try .exe
            binary_exe = binary.with_suffix(".exe")
            if binary_exe.exists():
                binary = binary_exe
        if binary.exists() and binary.is_file():
            proc = subprocess.run([str(binary)] + sys.argv[1:])
            return proc.returncode
    except Exception:
        pass
    # Fallback: invoke cargo-run style if developing
    try:
        proc = subprocess.run(["pyperfguard"] + sys.argv[1:])
        return proc.returncode
    except FileNotFoundError:
        try:
            repo_root = Path(__file__).resolve().parent
            cargo = ["cargo", "run", "--quiet", "--manifest-path", str(repo_root / "Cargo.toml"), "--"]
            proc = subprocess.run(cargo + sys.argv[1:])
            return proc.returncode
        except Exception:
            pass
    print("pyperfguard binary not found; install the Rust binary or rebuild", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
