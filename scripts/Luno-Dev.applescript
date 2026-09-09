-- Luno Dock-icon click target (source; built by scripts/install-dock-icon.sh).
-- Fires the detached launcher and returns immediately: the first click's
-- debug compile can take minutes, so nothing here waits for readiness.
-- __LUNO_REPO__ is substituted with the repository path at install time.
on run
	do shell script "nohup " & quoted form of "__LUNO_REPO__/scripts/luno-tray.sh" & " launch >/dev/null 2>&1 &"
end run
