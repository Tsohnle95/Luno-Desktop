"""Strict comparison and useful mismatch diagnostics."""

from __future__ import annotations

import json
from typing import Any


class DifferentialMismatch(AssertionError):
    pass


def compare_outputs(operation: str, reference: Any, target: Any) -> None:
    if reference != target:
        expected = json.dumps(reference, sort_keys=True, indent=2)
        actual = json.dumps(target, sort_keys=True, indent=2)
        raise DifferentialMismatch(
            f"{operation} mismatch\nreference:\n{expected}\ntarget:\n{actual}"
        )
