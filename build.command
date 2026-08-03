#!/bin/bash
cd "$(dirname "$0")"
source .venv/bin/activate
echo "=== Building Luno ==="
pyinstaller --clean --noconfirm Luno.spec
if [ $? -eq 0 ]; then
    echo "=== Build complete, launching ==="
    open dist/Luno.app
else
    echo "=== BUILD FAILED ==="
    exit 1
fi
