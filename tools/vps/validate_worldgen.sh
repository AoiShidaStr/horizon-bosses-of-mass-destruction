#!/bin/bash
# Run through /opt/build/queue.sh after rebuilding the content package.
set -euo pipefail

MOD=/opt/build/mods/bosses_of_mass_destruction
WT=/opt/build/wt/bosses_of_mass_destruction

cd "$WT"
cargo run -p cargo-horizon-mod -- check "$MOD/bosses_of_mass_destruction.hmod"
cargo run -p cargo-horizon-mod -- check "$MOD/bosses_of_mass_destruction.hnmod"
bash "$MOD/tools/vps/smoke_structures.sh"
bash "$MOD/tools/vps/smoke_locate.sh"
