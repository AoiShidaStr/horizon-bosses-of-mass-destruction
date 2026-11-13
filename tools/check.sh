#!/bin/sh
# Type-check the [api] crate against the Horizon source in ../../horizon-ref,
# without building the game. It copies the crate into .build/ws, points its
# horizon-client dependency at horizon-ref, and runs `cargo check` there with
# Horizon's dev profile, so the cached build in .build/target is reused.
#
#   sh tools/check.sh
#
# Runs with 2 jobs at the lowest priority (the PC may be in use for a game).
# This only type-checks. The real build is the launcher's
# `native-build mixins/bosses_of_mass_destruction`.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WS="$ROOT/.build/ws"
SRC="$ROOT/mixins/bosses_of_mass_destruction/api"
REF="$ROOT/../../horizon-ref"
mkdir -p "$WS"
cp "$REF/Cargo.lock" "$WS/Cargo.lock"
cat > "$WS/Cargo.toml" <<'EOF'
[workspace]
members = ["api"]
resolver = "2"

[patch.crates-io]
wgpu-hal = { path = "../../../../horizon-ref/third_party/wgpu-hal" }
naga = { path = "../../../../horizon-ref/third_party/naga" }
libmimalloc-sys = { path = "../../../../horizon-ref/third_party/libmimalloc-sys" }

[profile.dev]
debug = "line-tables-only"
[profile.dev.package.horizon-mod-api-gen]
opt-level = 3
[profile.dev.package.syn]
opt-level = 3
[profile.dev.package.proc-macro2]
opt-level = 3
EOF
rm -rf "$WS/api"
mkdir -p "$WS/api"
cp -r "$SRC/src" "$WS/api/src"
sed 's|^horizon-client = .*|horizon-client = { path = "../../../../../horizon-ref/crates/horizon-client" }|' \
    "$SRC/Cargo.toml" > "$WS/api/Cargo.toml"
cd "$WS"
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$ROOT/.build/target"
exec nice -n 19 cargo check -p mod-bosses_of_mass_destruction --message-format short "$@"
