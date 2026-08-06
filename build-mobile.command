#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
ANDROID_DIR="$ROOT_DIR/mobile-app"
APK_PATH="$ANDROID_DIR/app/build/outputs/apk/debug/app-debug.apk"

if [[ ! -x "$ANDROID_DIR/gradlew" ]]; then
    echo "=== Android Gradle wrapper not found or not executable ===" >&2
    exit 1
fi

echo "=== Building Luno Android debug APK ==="

if command -v adb >/dev/null 2>&1 && adb get-state >/dev/null 2>&1; then
    (
        cd "$ANDROID_DIR"
        ./gradlew :app:installDebug
    )

    echo "=== Build installed; launching Luno ==="
    adb shell monkey -p com.luno.mobile 1 >/dev/null 2>&1
    echo "APK: $APK_PATH"
else
    (
        cd "$ANDROID_DIR"
        ./gradlew :app:assembleDebug
    )

    echo "=== Build complete; no connected Android device detected ==="
    echo "APK: $APK_PATH"
fi
