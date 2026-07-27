#!/bin/bash
cd "$(dirname "$0")"
source .venv/bin/activate
echo "=== Building BoomTunes ==="
pyinstaller --clean --noconfirm DevTunes.spec
if [ $? -eq 0 ]; then
    echo "=== Build complete, launching ==="
    open dist/BoomTunes.app
else
    echo "=== BUILD FAILED ==="
    exit 1
fi
