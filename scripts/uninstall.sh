#!/usr/bin/env bash
# Restore the user's original selections without deleting immutable releases.
set -euo pipefail
umask 077
script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=install-common.sh
source "$script_dir/install-common.sh"
[[ $# = 0 ]] || die 'usage: scripts/uninstall.sh'
transaction=0
archive=
finish() {
  local status=$?
  trap - EXIT
  if [[ $transaction = 1 && -n $archive && ! -e $current && ! -L $current ]] &&
      receipt_matches "$archive/receipt" "$old_hash"; then
    transaction=0
  fi
  if [[ $transaction = 1 ]]; then
    printf 'Workstream: uninstall failed; restoring installed selections.\n' >&2
    select_release "$old_release" || status=1
    restart_service || printf 'Workstream: the renderer service still needs attention.\n' >&2
  fi
  cleanup_lock
  exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
init_paths
[[ -d $current && ! -L $current ]] || die 'no valid Workstream installation receipt was found.'
# Uninstall is also the recovery path for a failed or inactive new renderer.
read_release
check_owned_links "$old_release"
check_backups "$current"
transaction=1
restore_originals "$current"
restart_service || die 'the previous renderer did not start successfully.'
# Retain backups as a recovery receipt; no release is removed because original
# user symlinks may point at one of them (or optional hooks may still use one).
archive=$(mktemp -d "$state_root/uninstalled.XXXXXX")
mv -T -- "$current" "$archive/receipt"
transaction=0
printf 'Previous renderer and CLI selections restored; service is active. Optional hooks were not changed.\n'
