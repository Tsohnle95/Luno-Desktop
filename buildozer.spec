[app]

# (str) Title of your application
title = Luno

# (str) Package name
package.name = luno

# (str) Package domain (needed for android/ios packaging)
package.domain = com.ty.luno

# (str) Source code where the main.py lives
source.dir = .

# (list) Source files to include (let's include json, py, png, jpg)
source.include_exts = py,png,jpg,json

# (list) List of exclusions using pattern matching
#source.exclude_patterns = license,images/*png

# (list) List of directory to exclude
source.exclude_dirs = .venv, build, dist, __pycache__, .git, .agents

# (str) Application versioning (method 1)
version = 4.0.0

# (list) Application requirements
# comma separated e.g. requirements = sqlite3,kivy
requirements = python3,flet,flet-audio,mutagen,requests,pillow

# (str) Supported orientations
# Valid values are: landscape, portrait, portrait-upside-down, landscape-left, landscape-right
orientation = portrait

# (bool) Indicate if the application should be fullscreen or not
fullscreen = 0

# (list) Permissions
android.permissions = INTERNET, READ_EXTERNAL_STORAGE, WRITE_EXTERNAL_STORAGE, MANAGE_EXTERNAL_STORAGE

# (int) Android API to use
android.api = 33

# (int) Minimum API level
android.minapi = 21

# (int) Target SDK version (should match android.api)
android.sdk_version = 33

# (str) Android NDK version to use
# android.ndk = 25b

# (list) Pattern to exclude for shrink (e.g. *.png)
# android.exclude_obfuscation =

# (list) Architecture to build for (e.g. armeabi-v7a, arm64-v8a)
android.archs = arm64-v8a, armeabi-v7a

# (bool) Use --private data directory (True) or public /sdcard (False)
android.private_storage = True

[buildozer]
# (int) Log level (0 = error only, 1 = info, 2 = debug and stdout)
log_level = 2

# (int) Display warning if buildozer is run as root (0 = False, 1 = True)
warn_on_root = 1
