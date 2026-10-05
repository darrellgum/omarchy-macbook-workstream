#!/usr/bin/env bash
# Isolated integration tests: fake HOME, fake binaries, fake systemctl.
set -euo pipefail
(( EUID != 0 )) || { printf 'Run installer tests as an ordinary user.\n' >&2; exit 1; }
repo=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
suite=$(mktemp -d)
trap 'rm -rf -- "$suite"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
original_path=$PATH
export WORKSTREAM_REAL_MV
WORKSTREAM_REAL_MV=$(command -v mv)
names=(t1-touchbar-tokyo t1-workstream t1-workstream-codex t1-workstream-claude)

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_absent() { [[ ! -e $1 && ! -L $1 ]] || fail "expected absent: $1"; }
assert_link() { [[ -L $1 && $(readlink -- "$1") = "$2" ]] || fail "unexpected link: $1"; }
expect_failure() {
  if "$@" > "$case_dir/output" 2>&1; then fail "unexpected success: $*"; fi
}

make_build() {
  local version=$1 name
  for name in "${names[@]}"; do
    printf '#!/bin/sh\n# %s %s\nexit 0\n' "$name" "$version" > "$fixture/.build/bin/$name"
    chmod 755 "$fixture/.build/bin/$name"
  done
  (cd "$fixture/.build/bin" && sha256sum -- "${names[@]}") > "$fixture/.build/bin/SHA256SUMS"
}

new_case() {
  case_dir=$suite/$1
  fixture=$case_dir/repo
  mkdir -p "$fixture/scripts" "$fixture/.build/bin" "$case_dir/mock-bin"
  cp "$repo/scripts/install.sh" "$repo/scripts/uninstall.sh" "$repo/scripts/install-common.sh" "$fixture/scripts/"
  export HOME=$case_dir/home XDG_CONFIG_HOME=$case_dir/config XDG_STATE_HOME=$case_dir/state
  export WORKSTREAM_MOCK_STATE=$case_dir/mock
  export PATH=$case_dir/mock-bin:$original_path
  unset WORKSTREAM_MOCK_FAIL_RESTART WORKSTREAM_MOCK_INACTIVE WORKSTREAM_MOCK_INACTIVE_AFTER WORKSTREAM_MOCK_TERM_AFTER
  mkdir -p "$HOME" "$XDG_CONFIG_HOME/t1bridge" "$HOME/.local/bin"
  cat > "$case_dir/mock-bin/systemctl" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 = --user && ${*: -1} = t1-touchbar.service ]] || exit 90
case $2 in
  is-active)
    count=0
    [[ ! -f $WORKSTREAM_MOCK_STATE ]] || read -r count < "$WORKSTREAM_MOCK_STATE"
    [[ ${WORKSTREAM_MOCK_INACTIVE:-0} = 0 && $count != ${WORKSTREAM_MOCK_INACTIVE_AFTER:-none} ]]
    ;;
  restart)
    count=0
    [[ ! -f $WORKSTREAM_MOCK_STATE ]] || read -r count < "$WORKSTREAM_MOCK_STATE"
    count=$((count + 1))
    printf '%s\n' "$count" > "$WORKSTREAM_MOCK_STATE"
    [[ $count != ${WORKSTREAM_MOCK_FAIL_RESTART:-none} ]]
    ;;
  *) exit 91 ;;
esac
MOCK
  chmod 755 "$case_dir/mock-bin/systemctl"
  cat > "$case_dir/mock-bin/mv" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
"$WORKSTREAM_REAL_MV" "$@"
destination=${*: -1}
signal=0
case ${WORKSTREAM_MOCK_TERM_AFTER:-} in
  first-install) [[ $destination != "$XDG_STATE_HOME/t1-workstream/current" ]] || signal=1 ;;
  upgrade) [[ $destination != "$XDG_STATE_HOME/t1-workstream/current/release" ]] || signal=1 ;;
  uninstall) [[ $destination != "$XDG_STATE_HOME"/t1-workstream/uninstalled.*/receipt ]] || signal=1 ;;
esac
if [[ $signal = 1 ]]; then kill -TERM "$PPID"; fi
MOCK
  chmod 755 "$case_dir/mock-bin/mv"
  make_build one
  renderer=$XDG_CONFIG_HOME/t1bridge/renderer
  cli=$HOME/.local/bin/t1-workstream
  codex=$HOME/.local/bin/t1-workstream-codex
  claude=$HOME/.local/bin/t1-workstream-claude
  current=$XDG_STATE_HOME/t1-workstream/current
}

install_it() { bash "$fixture/scripts/install.sh" > "$case_dir/output" 2>&1; }
uninstall_it() { bash "$fixture/scripts/uninstall.sh" > "$case_dir/output" 2>&1; }

new_case restoration
ln -s '/missing/original renderer' "$renderer"
printf 'original CLI\n' > "$cli"
chmod 640 "$cli"
ln -s '../elsewhere/codex' "$codex"
install_it
release=$(dirname -- "$(readlink -- "$renderer")")
[[ -x $release/t1-workstream-claude ]] || fail 'sibling binaries missing'
install_it
[[ $(cat "$WORKSTREAM_MOCK_STATE") = 1 ]] || fail 'repeat installation restarted service'
make_build two
install_it
[[ $(dirname -- "$(readlink -- "$renderer")") != "$release" ]] || fail 'upgrade did not select new release'
uninstall_it
assert_link "$renderer" '/missing/original renderer'
[[ $(cat "$cli") = 'original CLI' && $(stat -c %a "$cli") = 640 ]] || fail 'regular file or mode not restored'
assert_link "$codex" '../elsewhere/codex'
assert_absent "$claude"
assert_absent "$current"
[[ -d $release ]] || fail 'old immutable release was removed'
printf 'PASS: original links/files/absence and modes restored after repeat install and upgrade\n'

new_case first_restart_failure
printf 'original renderer\n' > "$renderer"
export WORKSTREAM_MOCK_FAIL_RESTART=1
expect_failure install_it
[[ $(cat "$renderer") = 'original renderer' ]] || fail 'failed installation did not restore renderer'
assert_absent "$cli"
assert_absent "$codex"
assert_absent "$claude"
assert_absent "$current"
[[ $(cat "$WORKSTREAM_MOCK_STATE") = 2 ]] || fail 'failed install did not restart restored renderer'
printf 'PASS: first installation rolls back after restart failure\n'

new_case inactive_after_restart
export WORKSTREAM_MOCK_INACTIVE_AFTER=1
expect_failure install_it
assert_absent "$renderer"
assert_absent "$current"
[[ $(cat "$WORKSTREAM_MOCK_STATE") = 2 ]] || fail 'inactive renderer was not rolled back'
printf 'PASS: a successful restart command still requires an active service\n'

new_case upgrade_restart_failure
install_it
previous=$(readlink -- "$renderer")
previous_hash=$(cat "$current/release")
make_build two
export WORKSTREAM_MOCK_FAIL_RESTART=2
expect_failure install_it
assert_link "$renderer" "$previous"
[[ $(cat "$current/release") = "$previous_hash" ]] || fail 'failed upgrade changed receipt'
unset WORKSTREAM_MOCK_FAIL_RESTART
uninstall_it
assert_absent "$renderer"
printf 'PASS: failed upgrade preserves previous release and original restoration\n'

new_case uninstall_restart_failure
install_it
previous=$(readlink -- "$renderer")
export WORKSTREAM_MOCK_FAIL_RESTART=2
expect_failure uninstall_it
assert_link "$renderer" "$previous"
[[ -f $current/release ]] || fail 'failed uninstall lost receipt'
unset WORKSTREAM_MOCK_FAIL_RESTART
uninstall_it
assert_absent "$renderer"
printf 'PASS: failed uninstall reinstates Workstream and retains recovery receipt\n'

new_case inactive_uninstall
install_it
# Simulate a renderer that subsequently failed; restarting the original recovers.
export WORKSTREAM_MOCK_INACTIVE_AFTER=1
uninstall_it
assert_absent "$renderer"
assert_absent "$current"
[[ $(cat "$WORKSTREAM_MOCK_STATE") = 2 ]] || fail 'uninstall did not restart an inactive service'
printf 'PASS: uninstall restores and starts the original renderer when service is inactive\n'

new_case term_first_install
export WORKSTREAM_MOCK_TERM_AFTER=first-install
expect_failure install_it
installed_hash=$(cat "$current/release")
assert_link "$renderer" "$HOME/.local/libexec/t1-workstream/$installed_hash/t1-touchbar-tokyo"
unset WORKSTREAM_MOCK_TERM_AFTER
install_it
uninstall_it
assert_absent "$renderer"
printf 'PASS: TERM after first receipt commit leaves consistent, reversible installation\n'

new_case term_upgrade
printf 'original renderer\n' > "$renderer"
install_it
first_hash=$(cat "$current/release")
make_build two
export WORKSTREAM_MOCK_TERM_AFTER=upgrade
expect_failure install_it
installed_hash=$(cat "$current/release")
[[ $installed_hash != "$first_hash" ]] || fail 'upgrade receipt did not commit'
assert_link "$renderer" "$HOME/.local/libexec/t1-workstream/$installed_hash/t1-touchbar-tokyo"
unset WORKSTREAM_MOCK_TERM_AFTER
install_it
uninstall_it
[[ $(cat "$renderer") = 'original renderer' ]] || fail 'interrupted upgrade lost original backup'
printf 'PASS: TERM after upgrade receipt commit preserves matching selection and baseline\n'

new_case term_uninstall
printf 'original renderer\n' > "$renderer"
install_it
export WORKSTREAM_MOCK_TERM_AFTER=uninstall
expect_failure uninstall_it
assert_absent "$current"
[[ $(cat "$renderer") = 'original renderer' ]] || fail 'committed uninstall was rolled back after TERM'
unset WORKSTREAM_MOCK_TERM_AFTER
install_it
uninstall_it
[[ $(cat "$renderer") = 'original renderer' ]] || fail 'subsequent install could not preserve restored original'
printf 'PASS: TERM after uninstall receipt archive preserves completed restoration\n'

new_case killed_lock_holder
mkfifo "$case_dir/holder-input"
# Only shell builtins run after acquiring the lock, so the killed holder is the
# sole owner; no surviving child process can intentionally retain its descriptor.
bash -c 'set -euo pipefail; source "$1"; init_paths; exec 9<> "$2"; printf ready > "$3"; read -r -u 9' \
  bash "$fixture/scripts/install-common.sh" "$case_dir/holder-input" "$case_dir/ready" &
holder=$!
for ((attempt=0; attempt<100; attempt++)); do
  [[ ! -f $case_dir/ready ]] || break
  sleep 0.01
done
[[ -f $case_dir/ready ]] || { kill "$holder"; fail 'lock holder did not start'; }
expect_failure install_it
assert_absent "$renderer"
kill -KILL "$holder"
wait "$holder" 2>/dev/null || true
install_it
uninstall_it
printf 'PASS: concurrent install is refused and SIGKILL releases the lock automatically\n'

new_case conflict
install_it
previous=$(readlink -- "$renderer")
rm -- "$codex"
printf 'user replacement\n' > "$codex"
expect_failure uninstall_it
expect_failure install_it
assert_link "$renderer" "$previous"
[[ $(cat "$codex") = 'user replacement' && -f $current/release ]] || fail 'user change was clobbered'
[[ $(cat "$WORKSTREAM_MOCK_STATE") = 1 ]] || fail 'conflict restarted service'
printf 'PASS: user-edited selections preserved by install and uninstall\n'

new_case restored_release
install_it
retained=$(readlink -- "$renderer")
uninstall_it
ln -s -- "$retained" "$renderer"
make_build two
install_it
uninstall_it
assert_link "$renderer" "$retained"
[[ -x $renderer ]] || fail 'restored selection points at a removed release'
printf 'PASS: restored original selections may safely refer to retained releases\n'

new_case preflight
export WORKSTREAM_MOCK_INACTIVE=1
expect_failure install_it
assert_absent "$renderer"
assert_absent "$WORKSTREAM_MOCK_STATE"
unset WORKSTREAM_MOCK_INACTIVE
printf 'tampered\n' >> "$fixture/.build/bin/t1-workstream"
expect_failure install_it
assert_absent "$renderer"
assert_absent "$current"
printf 'PASS: inactive service and mismatched checksums rejected before changing selections\n'

printf 'All isolated installer tests passed.\n'
