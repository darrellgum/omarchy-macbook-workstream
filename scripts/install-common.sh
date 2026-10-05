#!/usr/bin/env bash
# Shared user-install transaction helpers. This file is sourced, not executed.

names=(t1-touchbar-tokyo t1-workstream t1-workstream-codex t1-workstream-claude)

die() { printf 'Workstream: %s\n' "$*" >&2; exit 1; }

init_paths() {
  (( EUID != 0 )) || die 'run as your desktop user, without sudo.'
  [[ ${HOME:-} = /* ]] || die 'HOME must be an absolute path.'
  config_home=${XDG_CONFIG_HOME:-$HOME/.config}
  state_home=${XDG_STATE_HOME:-$HOME/.local/state}
  [[ $config_home = /* && $state_home = /* ]] || die 'XDG paths must be absolute.'
  state_root=$state_home/t1-workstream
  current=$state_root/current
  releases=$HOME/.local/libexec/t1-workstream
  paths=("$config_home/t1bridge/renderer" "$HOME/.local/bin/t1-workstream"
    "$HOME/.local/bin/t1-workstream-codex" "$HOME/.local/bin/t1-workstream-claude")
  mkdir -p -- "$state_root"
  chmod 700 -- "$state_root"
  # Keep this inode in place: unlinking a flock file can create independent locks.
  # The kernel releases the lock when its descriptors close, including on SIGKILL.
  exec {lock_fd}>> "$state_root/install.lock"
  flock -n "$lock_fd" || die 'another installer is running.'
}

cleanup_lock() {
  if [[ -n ${lock_fd:-} ]]; then exec {lock_fd}>&-; fi
}

service_active() { systemctl --user is-active --quiet t1-touchbar.service; }
restart_service() {
  systemctl --user restart t1-touchbar.service && service_active
}

require_service() {
  service_active || die 't1-touchbar.service must already be active; set up and start T1Bridge first.'
}

read_release() {
  [[ -f $current/release && ! -L $current/release ]] || die 'installation receipt is missing or invalid.'
  IFS= read -r old_hash < "$current/release"
  [[ $old_hash =~ ^[0-9a-f]{64}$ ]] || die 'installation receipt contains an invalid release hash.'
  old_release=$releases/$old_hash
}

# Renaming a verified receipt is the transaction's commit point. A signal may
# arrive after mv completes but before the next shell assignment runs.
receipt_matches() {
  local directory=$1 expected=$2 recorded
  [[ -d $directory && ! -L $directory && -f $directory/release && ! -L $directory/release ]] || return 1
  IFS= read -r recorded < "$directory/release" || return 1
  [[ $recorded = "$expected" ]]
}

check_owned_links() {
  local release=$1 i
  for i in "${!paths[@]}"; do
    [[ -L ${paths[i]} && $(readlink -- "${paths[i]}") = "$release/${names[i]}" ]] ||
      die "selection changed since installation; preserving it: ${paths[i]}"
  done
}

check_backups() {
  local receipt=$1 i kind
  for i in "${!paths[@]}"; do
    [[ -f $receipt/$i.kind && ! -L $receipt/$i.kind ]] || die 'original-state receipt is incomplete.'
    IFS= read -r kind < "$receipt/$i.kind"
    case $kind in
      absent) ;;
      file) [[ -f $receipt/$i.saved && ! -L $receipt/$i.saved ]] || die 'original file backup is missing.' ;;
      link) [[ -L $receipt/$i.saved ]] || die 'original symlink backup is missing.' ;;
      *) die 'original-state receipt is invalid.' ;;
    esac
  done
}

save_originals() {
  local receipt=$1 i kind
  for i in "${!paths[@]}"; do
    if [[ -L ${paths[i]} ]]; then kind=link
    elif [[ -f ${paths[i]} ]]; then kind=file
    elif [[ -e ${paths[i]} ]]; then die "refusing unsupported path type: ${paths[i]}"
    else kind=absent
    fi
    printf '%s\n' "$kind" > "$receipt/$i.kind"
    if [[ $kind != absent ]]; then cp -a -- "${paths[i]}" "$receipt/$i.saved"; fi
  done
}

# Stage beside the destination so rename stays atomic even across filesystems.
atomic_link() {
  local target=$1 destination=$2 stage
  mkdir -p -- "$(dirname -- "$destination")"
  stage=$(mktemp -d "$(dirname -- "$destination")/.workstream-link.XXXXXX")
  if ln -s -- "$target" "$stage/link" && mv -Tf -- "$stage/link" "$destination"; then
    rmdir -- "$stage"
  else
    rm -rf -- "$stage"
    return 1
  fi
}

select_release() {
  local release=$1 i
  for i in "${!paths[@]}"; do atomic_link "$release/${names[i]}" "${paths[i]}" || return 1; done
}

restore_originals() {
  local receipt=$1 i kind stage
  for i in "${!paths[@]}"; do
    IFS= read -r kind < "$receipt/$i.kind"
    if [[ $kind = absent ]]; then
      rm -f -- "${paths[i]}" || return 1
    else
      stage=$(mktemp -d "$(dirname -- "${paths[i]}")/.workstream-restore.XXXXXX") || return 1
      if cp -a -- "$receipt/$i.saved" "$stage/original" && mv -Tf -- "$stage/original" "${paths[i]}"; then
        rmdir -- "$stage"
      else
        rm -rf -- "$stage"
        return 1
      fi
    fi
  done
}
