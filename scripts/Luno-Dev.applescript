-- Luno Dock-icon click target (source; built by scripts/install-dock-icon.sh).
-- Fires the detached launcher and returns immediately. It runs `restart`, so
-- every click stops any running dev server and starts a fresh DEBUG build —
-- click to rebuild + relaunch after a code change, no terminal needed.
-- __LUNO_REPO__ is substituted with the repository path at install time.
on run
	do shell script "nohup " & quoted form of "__LUNO_REPO__/scripts/luno-tray.sh" & " restart >/dev/null 2>&1 &"
end run
