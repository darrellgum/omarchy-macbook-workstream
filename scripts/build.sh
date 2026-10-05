#!/usr/bin/env bash
# Fetch a pinned T1Bridge revision and build only the optional user-space tools.
set -euo pipefail
umask 022
[[ $# = 0 ]] || { printf 'usage: scripts/build.sh\n' >&2; exit 1; }
(( EUID != 0 )) || { printf 'Build as your desktop user, without sudo.\n' >&2; exit 1; }
repo=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
revision=81cbdf81026a16e02f0bea74735c6b029a8ffae2
for command in git cargo rustc cc ar sha256sum install mktemp flock; do
  command -v "$command" >/dev/null || { printf 'Missing build dependency: %s\n' "$command" >&2; exit 1; }
done
mkdir -p -- "$repo/.build"
exec 9>"$repo/.build/build.lock"
flock -n 9 || { printf 'Another Workstream build is running.\n' >&2; exit 1; }
source_dir=$(mktemp -d "$repo/.build/source.XXXXXX")
printf 'Preparing T1Bridge %s in %s\n' "$revision" "$source_dir"
git -C "$source_dir" init --quiet
git -C "$source_dir" fetch --quiet --depth=1 https://github.com/standardagents/t1bridge.git "$revision"
git -C "$source_dir" checkout --quiet --detach FETCH_HEAD
[[ $(git -C "$source_dir" rev-parse HEAD) = "$revision" ]]
git -C "$source_dir" apply --check "$repo/workstream/patches/0001-workstream.patch"
git -C "$source_dir" apply "$repo/workstream/patches/0001-workstream.patch"
export CARGO_TARGET_DIR="$repo/.build/target"
(cd -- "$source_dir" && cargo build --release --locked -p t1-touchbar \
  --features workstream-codex,workstream-claude \
  --bin t1-touchbar-tokyo --bin t1-workstream \
  --bin t1-workstream-codex --bin t1-workstream-claude)
names=(t1-touchbar-tokyo t1-workstream t1-workstream-codex t1-workstream-claude)
output=$(mktemp -d "$repo/.build/bin.XXXXXX")
for name in "${names[@]}"; do
  install -m 755 -- "$CARGO_TARGET_DIR/release/$name" "$output/$name"
done
(cd -- "$output" && sha256sum -- "${names[@]}" > SHA256SUMS)
# Preserve prior output; an interrupted build never removes the last build.
if [[ -e $repo/.build/bin || -L $repo/.build/bin ]]; then
  previous=$(mktemp -d "$repo/.build/previous.XXXXXX")
  mv -T -- "$repo/.build/bin" "$previous/bin"
fi
mv -T -- "$output" "$repo/.build/bin"
printf '%s\n' "$source_dir" > "$repo/.build/source-path"
printf 'Built four user-space tools. Review them, then run scripts/install.sh.\n'
