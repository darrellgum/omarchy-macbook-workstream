#!/usr/bin/env bash
# Install an explicitly built Workstream release for the current desktop user.
set -euo pipefail
umask 077
script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=install-common.sh
source "$script_dir/install-common.sh"
[[ $# = 0 ]] || die 'usage: scripts/install.sh (first run scripts/build.sh).'

transaction=0
staging=
release_stage=
receipt=
old_release=
finish() {
  local status=$?
  trap - EXIT
  if [[ $transaction = 1 ]] && receipt_matches "$current" "$hash"; then
    transaction=0
  fi
  if [[ $transaction = 1 ]]; then
    printf 'Workstream: installation failed; restoring previous selections.\n' >&2
    if [[ -n $old_release ]]; then select_release "$old_release" || status=1
    elif ! restore_originals "$receipt"; then
      printf 'Workstream: original backups retained for recovery at %s\n' "$receipt" >&2
      receipt=
      status=1
    fi
    restart_service || printf 'Workstream: previous selections restored, but the service still needs attention.\n' >&2
  fi
  [[ -z $staging ]] || rm -rf -- "$staging"
  [[ -z $release_stage ]] || rm -rf -- "$release_stage"
  [[ -z $receipt ]] || rm -rf -- "$receipt"
  cleanup_lock
  exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
init_paths
require_service

build=$script_dir/../.build/bin
[[ -f $build/SHA256SUMS && ! -L $build/SHA256SUMS ]] || die 'build output is missing; run scripts/build.sh first.'
staging=$(mktemp -d "$state_root/build-check.XXXXXX")
for name in "${names[@]}"; do
  [[ -f $build/$name && ! -L $build/$name && -x $build/$name ]] || die "missing executable: $name"
done
(cd -- "$build" && sha256sum -- "${names[@]}") | LC_ALL=C sort > "$staging/expected"
LC_ALL=C sort -- "$build/SHA256SUMS" > "$staging/provided"
cmp -s -- "$staging/expected" "$staging/provided" || die 'build checksums do not match the four expected binaries.'
hash=$(sha256sum -- "$staging/expected")
hash=${hash%% *}
release=$releases/$hash
mkdir -p -- "$releases"
if [[ -e $release || -L $release ]]; then
  [[ -d $release && ! -L $release ]] || die 'release path is not a directory.'
  for name in "${names[@]}"; do
    [[ -f $release/$name && ! -L $release/$name && -x $release/$name ]] || die 'existing release has an invalid executable.'
  done
  (cd -- "$release" && sha256sum -- "${names[@]}") | LC_ALL=C sort > "$staging/existing"
  cmp -s -- "$staging/expected" "$staging/existing" || die 'existing immutable release has changed.'
else
  release_stage=$(mktemp -d "$releases/.install.XXXXXX")
  # Include release staging in cleanup until its atomic rename completes.
  for name in "${names[@]}"; do install -m 755 -- "$build/$name" "$release_stage/$name"; done
  (cd -- "$release_stage" && sha256sum -- "${names[@]}") | LC_ALL=C sort > "$staging/copied"
  cmp -s -- "$staging/expected" "$staging/copied" || die 'build changed during installation.'
  cp -- "$staging/expected" "$release_stage/SHA256SUMS"
  mv -T -- "$release_stage" "$release"
  release_stage=
fi

if [[ -e $current || -L $current ]]; then
  [[ -d $current && ! -L $current ]] || die 'installation receipt is not a directory.'
  read_release
  check_owned_links "$old_release"
  check_backups "$current"
  if [[ $release = "$old_release" ]]; then
    printf 'Workstream is already installed and its service is active.\n'
    exit 0
  fi
else
  receipt=$(mktemp -d "$state_root/originals.XXXXXX")
  save_originals "$receipt"
fi
transaction=1
select_release "$release"
restart_service || die 'new renderer did not start successfully.'
if [[ -n $receipt ]]; then
  printf '%s\n' "$hash" > "$receipt/release"
  mv -T -- "$receipt" "$current"
  receipt=
else
  printf '%s\n' "$hash" > "$current/release.new"
  mv -f -- "$current/release.new" "$current/release"
fi
transaction=0
printf 'Workstream installed; t1-touchbar.service is active. Hooks remain a separate opt-in step.\n'
