#!/bin/bash

luno_configure_dev_path() {
  local repo="$1"
  local node_major candidate
  node_major="$(tr -d '[:space:]' < "$repo/.node-version")"
  node_major="${node_major%%.*}"

  PATH="$HOME/.cargo/bin:$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$repo/node_modules/.bin:$PATH"
  for candidate in \
    "$HOME"/.local/share/orbit-toolchains/node-v"$node_major".*/bin \
    "$HOME"/.nvm/versions/node/v"$node_major".*/bin; do
    if [[ -x "$candidate/node" && -x "$candidate/npm" ]]; then
      PATH="$candidate:$PATH"
      break
    fi
  done
  export PATH
}

luno_require_node() {
  local repo="$1"
  local required_major actual_major
  required_major="$(tr -d '[:space:]' < "$repo/.node-version")"
  required_major="${required_major%%.*}"

  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    echo "Node.js $required_major and npm are required. Install the version from $repo/.node-version." >&2
    return 1
  fi

  actual_major="$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null)" || actual_major=""
  if [[ "$actual_major" != "$required_major" ]]; then
    echo "Node.js $required_major is required by $repo/.node-version; found $(node --version 2>/dev/null || echo unavailable)." >&2
    return 1
  fi
}
