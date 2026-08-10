import json
from pathlib import Path

import pytest

from library_transfer import ManifestError, validate_manifest


FIXTURES = Path(__file__).parent / "contracts" / "library-export" / "v1"


def test_valid_shared_manifest_fixture_is_accepted():
    value = json.loads((FIXTURES / "valid.json").read_text(encoding="utf-8"))

    normalized = validate_manifest(value)

    assert normalized["format"] == "luno.library.export"
    assert normalized["manifest_version"] == 1
    assert normalized["tracks"][1]["ambiguity_confirmation_required"] is True


def test_invalid_shared_manifest_fixture_is_rejected():
    value = json.loads(
        (FIXTURES / "invalid-unknown-field.json").read_text(encoding="utf-8")
    )

    with pytest.raises(ManifestError):
        validate_manifest(value)
