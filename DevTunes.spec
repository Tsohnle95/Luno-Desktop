# -*- mode: python ; coding: utf-8 -*-

a = Analysis(
    ['music_player.py'],
    pathex=['/Users/ty/boombastic/.venv/lib/python3.12/site-packages'],
    binaries=[],
    datas=[('views', 'views')],
    hiddenimports=[
        'Foundation', 'objc',
        'Quartz', 'AppKit', 'ApplicationServices', 'MediaPlayer', 'AVFoundation',
        'requests', 'urllib3',
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='BoomTunes',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['icon.icns'],
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='BoomTunes',
)
app = BUNDLE(
    coll,
    name='BoomTunes.app',
    icon='icon.icns',
    bundle_identifier='com.ty.boomtunes',
)
