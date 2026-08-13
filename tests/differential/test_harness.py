import pytest

from tools.differential.comparator import DifferentialMismatch, compare_outputs
from tools.differential.reference import evaluate


def test_reference_operations_cover_frozen_pure_behavior():
    assert evaluate("parse_track_name", {"path": "/x/Artist - Song - Live.mp3"}) == {"title": "Song - Live", "artist": "Artist"}
    assert evaluate("normalize_duplicate", {"text": "Artist - Song (Official Audio)"}) == {"key": "artistsong"}
    assert evaluate("duration_match", {"expected_ms": 100_000, "actual_ms": 109_999}) == {"match": True}
    assert evaluate("duration_match", {"expected_ms": 100_000, "actual_ms": 110_001}) == {"match": False}


def test_scan_order_and_duplicates_are_explicit():
    result = evaluate("scan_order", {"entries": [
        {"path": "/r/b.mp3", "playlist": "Unsorted", "mtime": 1},
        {"path": "/r/a.mp3", "playlist": "Unsorted", "mtime": 2},
        {"path": "/r/z.mp3", "playlist": "Mix", "mtime": 0},
    ]})
    assert result == {"Unsorted": ["/r/a.mp3", "/r/b.mp3"], "Mix": ["/r/z.mp3"]}
    groups = evaluate("duplicates", {"library": {"A": ["/x/Artist - Song.mp3"], "B": ["/y/Artist - Song (Live).mp3"]}})
    assert len(groups["groups"]) == 1


def test_queue_modes_and_mismatch_fail_strictly():
    value = {"queue": ["current", "tail"], "selected": ["a", "b"], "queue_idx": 0, "user_queue_count": 0}
    assert evaluate("queue_insert", {**value, "mode": "play_next"})["queue"] == ["current", "a", "b", "tail"]
    assert evaluate("queue_insert", {**value, "mode": "add_queue"})["queue"] == ["current", "a", "b", "tail"]
    with pytest.raises(DifferentialMismatch):
        compare_outputs("parse_track_name", {"title": "a"}, {"title": "b"})
