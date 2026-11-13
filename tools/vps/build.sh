#!/bin/bash
# Sync BoMD's api crate into the worktree, test it, build the server engine
# (launcher-equivalent native-game package, --no-default-features), check the
# client flavour, and build horizon-bots for the smoke test.
. ~/.cargo/env
export CARGO_TARGET_DIR=/opt/build/target CARGO_BUILD_JOBS=4
WT=/opt/build/wt/bosses_of_mass_destruction
SRC=/opt/build/mods/bosses_of_mass_destruction/mixins/bosses_of_mass_destruction/api/src
rm -rf $WT/native-api/bosses_of_mass_destruction/src && cp -r $SRC $WT/native-api/bosses_of_mass_destruction/src
cd $WT
date
echo "== tests"; nice -n 10 cargo test -p mod-bosses_of_mass_destruction --message-format short 2>&1 | grep -vE "^\s+(Compiling|Checking)" | grep -E "test result|FAILED|panicked|error|warning: unused" ; echo "test_exit=${PIPESTATUS[0]}"
echo "== server engine"; nice -n 10 cargo build -p horizon-native-game --no-default-features --message-format short 2>&1 | grep -vE "^\s+(Compiling|Checking)"; echo "build_exit=${PIPESTATUS[0]}"
cp /opt/build/target/debug/horizon-client /opt/build/bomd-engine/horizon-engine && ls -la /opt/build/bomd-engine
echo "== client flavour check"; nice -n 10 cargo check -p horizon-native-game --message-format short 2>&1 | grep -vE "^\s+(Compiling|Checking)"; echo "check_exit=${PIPESTATUS[0]}"
echo "== horizon-bots"; nice -n 10 cargo build -p horizon-bots --message-format short 2>&1 | grep -vE "^\s+(Compiling|Checking)"; echo "bots_exit=${PIPESTATUS[0]}"
date; echo DONE
