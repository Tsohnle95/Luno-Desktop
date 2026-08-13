"""Python-versus-target differential harness for pure desktop behavior."""

from .comparator import DifferentialMismatch, compare_outputs
from .reference import evaluate

__all__ = ["DifferentialMismatch", "compare_outputs", "evaluate"]
