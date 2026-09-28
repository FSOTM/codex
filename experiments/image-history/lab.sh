#!/usr/bin/env bash
set -euo pipefail
lab_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
lab_mode="${1:-}"
if [[ "$lab_mode" != "patched" && "$lab_mode" != "baseline" ]]; then
  echo 'Usage: bash lab.sh {patched|baseline} [Codex CLI arguments...]' >&2
  exit 2
fi
shift
# Separate homes prevent auth, settings, history, and SQLite writes to the desktop home.
lab_home="$lab_root/homes/$lab_mode"
mkdir -p "$lab_home" "$lab_root/workspace"
chmod 700 "$lab_home"
lab_budget="${CODEX_LAB_IMAGE_BUDGET_BYTES:-8388608}"
unset CODEX_CLI_PATH CODEX_LAB_IMAGE_BUDGET_BYTES
export CODEX_HOME="$lab_home"
if [[ "$lab_mode" == "patched" ]]; then
  export CODEX_LAB_IMAGE_BUDGET_BYTES="$lab_budget"
fi
echo "Experimental CLI ($lab_mode), CODEX_HOME=$CODEX_HOME" >&2
cd "$lab_root/workspace"
exec "$lab_root/bin/codex" "$@"
