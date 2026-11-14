#!/bin/bash
# Run through /opt/build/queue.sh so native builds share the VPS FIFO lock.
set -euo pipefail

MOD=/opt/build/mods/bosses_of_mass_destruction
WT=/opt/build/wt/bosses_of_mass_destruction
BUILD_LOG=/opt/build/bomd-validation-build.log

bash "$MOD/tools/vps/build.sh" > "$BUILD_LOG" 2>&1
for check in test_exit build_exit check_exit bots_exit; do
  grep -Fx "$check=0" "$BUILD_LOG" || {
    cat "$BUILD_LOG"
    echo "VPS build stage failed: $check"
    exit 1
  }
done
grep -E 'test result|_exit=' "$BUILD_LOG"

cd "$WT"
cargo run -p cargo-horizon-mod -- check "$MOD/bosses_of_mass_destruction.hmod"
cargo run -p cargo-horizon-mod -- check "$MOD/bosses_of_mass_destruction.hnmod"

bash "$MOD/tools/vps/smoke.sh"
bash "$MOD/tools/vps/smoke_altar.sh"
bash "$MOD/tools/vps/smoke_structures.sh"
bash "$MOD/tools/vps/smoke_locate.sh"
