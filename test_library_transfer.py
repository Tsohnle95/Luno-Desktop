import json
import os
import tempfile
import unittest

from library_transfer import (
    MANIFEST_FORMAT,
    ManifestError,
    apply_import_plan,
    build_manifest,
    plan_import,
    validate_manifest,
)


class LibraryTransferTest(unittest.TestCase):
    def test_library_manifest_contains_no_local_paths_and_preserves_order(self):
        with tempfile.TemporaryDirectory() as root:
            playlist = os.path.join(root, "Favorites")
            os.makedirs(playlist)
            first = os.path.join(playlist, "Artist - First.mp3")
            second = os.path.join(playlist, "Artist - Second.mp3")
            open(first, "wb").close()
            open(second, "wb").close()

            manifest = build_manifest(
                {"Favorites": [first, second], "All Music": [first]},
                root,
                created_at="2026-08-02T00:00:00Z",
            )

            self.assertEqual(manifest["format"], MANIFEST_FORMAT)
            self.assertEqual(manifest["manifest_version"], 1)
            self.assertEqual([t["title"] for t in manifest["tracks"]], ["First", "Second"])
            self.assertEqual(manifest["playlists"][0]["track_refs"], [
                manifest["tracks"][0]["ref"],
                manifest["tracks"][1]["ref"],
            ])
            serialized = json.dumps(manifest)
            self.assertNotIn(first, serialized)
            self.assertNotIn(root, serialized)

    def test_selected_manifest_is_unassigned_and_marks_ambiguity(self):
        with tempfile.TemporaryDirectory() as root:
            path = os.path.join(root, "Artist - Song.mp3")
            open(path, "wb").close()
            manifest = build_manifest(
                {"Unsorted": [path]}, root, scope="selected_tracks", selected_paths=[path]
            )
            self.assertEqual(manifest["playlists"], [])
            self.assertEqual(len(manifest["unassigned_track_refs"]), 1)
            self.assertTrue(manifest["tracks"][0]["ambiguity_confirmation_required"])

    def test_validator_rejects_unknown_fields_and_path_traversal(self):
        base = {
            "format": MANIFEST_FORMAT,
            "manifest_version": 1,
            "scope": "library",
            "created_at": "2026-08-02T00:00:00Z",
            "tracks": [
                {
                    "ref": "trk:one",
                    "title": "Song",
                    "artist": "Artist",
                    "album": "",
                    "duration_ms": 0,
                    "source": None,
                    "ambiguity_confirmation_required": True,
                }
            ],
            "playlists": [{"name": "../escape", "description": "", "track_refs": ["trk:one"]}],
            "unassigned_track_refs": [],
        }
        with self.assertRaises(ManifestError):
            validate_manifest({**base, "secret": "nope"})
        with self.assertRaises(ManifestError):
            validate_manifest(base)

    def test_import_matches_metadata_and_copies_without_deleting(self):
        with tempfile.TemporaryDirectory() as root:
            source_dir = os.path.join(root, "Existing")
            os.makedirs(source_dir)
            source = os.path.join(source_dir, "Artist - Song.mp3")
            open(source, "wb").close()
            manifest = build_manifest({"Shared": [source]}, root, created_at="2026-08-02T00:00:00Z")
            plan = plan_import(manifest, {"Existing": [source]})
            result = apply_import_plan(plan, root)
            self.assertEqual(result["missing"], [])
            self.assertTrue(os.path.exists(os.path.join(root, "Shared", "Artist - Song.mp3")))
            self.assertTrue(os.path.exists(source))


if __name__ == "__main__":
    unittest.main()
