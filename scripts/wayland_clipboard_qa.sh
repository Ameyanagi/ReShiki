#!/usr/bin/env bash
set -euo pipefail

# Only disposable nested/headless compositors are started. Client DISPLAY is
# unset and a fresh D-Bus/session/data directory prevents desktop clipboard use.
if [[ "$(uname -s)" != Linux || $# != 3 ]]; then
  echo "Usage (Linux): $0 gnome|sway QA_BINARY NEW_OUTPUT_DIRECTORY" >&2
  exit 2
fi
if [[ -e "$3" ]]; then
  echo "The output directory must be new: $3" >&2
  exit 2
fi
if [[ -z "${RESHIKI_QA_SOURCE_HEAD:-}" ]]; then
  export RESHIKI_QA_SOURCE_HEAD
  RESHIKI_QA_SOURCE_HEAD="$(git rev-parse HEAD)"
fi
exec dbus-run-session -- /usr/bin/python3 native/linux/tests/wayland_session.py "$1" "$2" "$3"
