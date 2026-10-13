#!/bin/sh
# Probe Linux clipboard tooling. Probe only: always exits 0.
# Usage: bash scripts/check-linux-clipboard.sh

check() {
  if command -v "$1" >/dev/null 2>&1; then
    echo "OK: $1 found ($(command -v "$1"))"
  else
    echo "MISSING: $1 not found (hint: $2)"
  fi
}

check xclip "sudo apt-get install -y xclip"
check wl-paste "sudo apt-get install -y wl-clipboard"

if [ -n "$WAYLAND_DISPLAY" ]; then
  echo "OK: WAYLAND_DISPLAY=$WAYLAND_DISPLAY (Wayland session)"
else
  echo "INFO: WAYLAND_DISPLAY unset (likely X11)"
fi

if [ -n "$XDG_SESSION_TYPE" ]; then
  echo "INFO: XDG_SESSION_TYPE=$XDG_SESSION_TYPE"
else
  echo "INFO: XDG_SESSION_TYPE unset"
fi

exit 0
