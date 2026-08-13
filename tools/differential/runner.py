"""Run JSON differential cases against Python and an external target adapter.

Case format: ``{"operation": ..., "input": ...}``. The target command receives
one JSON object per line on stdin and must emit one JSON result per line.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from .comparator import compare_outputs
from .reference import evaluate


def run(cases: list[dict], target_command: list[str]) -> int:
    target = subprocess.Popen(target_command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    assert target.stdin is not None and target.stdout is not None
    failures = 0
    try:
        for number, case in enumerate(cases, 1):
            reference = evaluate(case["operation"], case["input"])
            target.stdin.write(json.dumps(case) + "\n")
            target.stdin.flush()
            line = target.stdout.readline()
            if not line:
                raise RuntimeError(f"target exited without output for case {number}")
            try:
                actual = json.loads(line)
            except json.JSONDecodeError as exc:
                raise RuntimeError(f"target emitted invalid JSON for case {number}: {exc}") from exc
            try:
                compare_outputs(case["operation"], reference, actual)
            except AssertionError as exc:
                print(f"case {number}: {exc}", file=sys.stderr)
                failures += 1
    finally:
        target.stdin.close()
        target.terminate()
        target.wait()
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("cases", type=Path)
    parser.add_argument("--target-command", nargs="+", required=True)
    args = parser.parse_args()
    cases = json.loads(args.cases.read_text(encoding="utf-8"))
    if not isinstance(cases, list):
        raise ValueError("case file must contain a JSON array")
    return run(cases, args.target_command)


if __name__ == "__main__":
    raise SystemExit(main())
