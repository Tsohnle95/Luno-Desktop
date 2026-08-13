"""Receipt rules shared by the migration verification suites.

An unavailable target gate is evidence about the environment, not a success.
Keep this module dependency-free so it can run before the target is packaged.
"""

from dataclasses import dataclass


VALID_STATUSES = {"pass", "fail", "inconclusive"}


@dataclass(frozen=True)
class Receipt:
    acceptance: str
    status: str
    command: str
    detail: str

    def __post_init__(self):
        if self.status not in VALID_STATUSES:
            raise ValueError(f"invalid receipt status: {self.status}")
        if self.status == "pass" and any(word in self.detail.lower() for word in ("skip", "unavailable", "timeout", "truncated", "inconclusive")):
            raise AssertionError(f"inconclusive evidence cannot be marked pass: {self.acceptance}")


def unavailable(acceptance: str, command: str, detail: str) -> Receipt:
    return Receipt(acceptance, "inconclusive", command, detail)
