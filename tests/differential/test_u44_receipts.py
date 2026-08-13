import os
import subprocess
import sys
import time
from pathlib import Path

from u44_receipts import Receipt, unavailable


def _fake_tool(path: Path, body: str) -> Path:
    path.write_text(f"#!/bin/sh\n{body}\n")
    path.chmod(0o755)
    return path


def test_fake_yt_dlp_and_ffmpeg_have_deterministic_terminal_signals(tmp_path):
    yt = _fake_tool(tmp_path / "yt-dlp", 'printf \'{"id":"fixture-video-1"}\'')
    ffmpeg = _fake_tool(tmp_path / "ffmpeg", "printf audio > \"$2\"")
    metadata = subprocess.run([str(yt)], check=True, capture_output=True, text=True)
    output = tmp_path / "audio.mp3"
    subprocess.run([str(ffmpeg), "-o", str(output)], check=True)
    assert metadata.stdout == '{"id":"fixture-video-1"}'
    assert output.read_bytes() == b"audio"


def test_fake_process_timeout_is_not_a_pass(tmp_path):
    sleeper = _fake_tool(tmp_path / "slow-process", "sleep 2")
    started = time.monotonic()
    try:
        subprocess.run([str(sleeper)], timeout=0.05, check=True)
    except subprocess.TimeoutExpired:
        elapsed = time.monotonic() - started
        receipt = unavailable("AC-OPS-03", "fake process timeout", f"timeout after {elapsed:.3f}s")
        assert receipt.status == "inconclusive"
    else:
        raise AssertionError("fake process unexpectedly completed")


def test_receipt_rejects_inconclusive_words_in_pass_detail():
    try:
        Receipt("AC-SHELL-01", "pass", "not-run", "browser unavailable")
    except AssertionError:
        pass
    else:
        raise AssertionError("receipt allowed unavailable evidence to pass")
