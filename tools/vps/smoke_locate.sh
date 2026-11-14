#!/bin/bash
# Verify all four structure sets can locate a naturally eligible jigsaw start
# in a fresh world and in their intended dimensions. Uses only scratch data.
set -euo pipefail

S=/opt/build/bomd-smoke
SRV=$S/server-locate
ENGINE=${ENGINE:-/opt/build/bomd-engine/horizon-engine}
LOG=$S/locate-engine.log
IN=$S/locate-console.in
PORT=25633

rm -rf "$SRV"
mkdir -p "$SRV/horizon-mods"
cp /opt/build/bosses_of_mass_destruction.hmod "$SRV/horizon-mods/"
printf 'mod.bosses_of_mass_destruction:true\n' > "$SRV/horizon-mods.txt"
printf 'eula=true\n' > "$SRV/eula.txt"
cat > "$SRV/server.properties" <<P
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-type=minecraft\:normal
level-seed=20261114
gamemode=creative
difficulty=normal
spawn-protection=0
pause-when-empty-seconds=0
view-distance=4
simulation-distance=4
generate-structures=true
spawn-monsters=false
spawn-animals=false
P
rm -f "$IN"
mkfifo "$IN"
: > "$LOG"
cd "$SRV"
export HORIZON_NO_AUDIO=1
"$ENGINE" --resources "$S/resources" --dedicated --nogui < "$IN" > "$LOG" 2>&1 &
ENGINE_PID=$!
exec 3>"$IN"
cleanup() {
  if kill -0 "$ENGINE_PID" 2>/dev/null; then
    printf 'stop\n' >&3 || true
    for _ in $(seq 1 30); do
      kill -0 "$ENGINE_PID" 2>/dev/null || break
      sleep 1
    done
    kill "$ENGINE_PID" 2>/dev/null || true
    wait "$ENGINE_PID" 2>/dev/null || true
  fi
  exec 3>&-
  rm -f "$IN"
}
trap cleanup EXIT

for i in $(seq 1 240); do
  grep -q 'Done (' "$LOG" && break
  kill -0 "$ENGINE_PID" 2>/dev/null || { cat "$LOG"; exit 1; }
  sleep 1
done
grep -q 'Done (' "$LOG" || { cat "$LOG"; exit 1; }

locate() {
  local dimension=$1 name=$2 marker="BOMD_LOCATE_${2}"
  local line response
  line=$(wc -l < "$LOG")
  printf 'execute in %s run locate structure bosses_of_mass_destruction:%s_shrine\n' \
    "$dimension" "$name" >&3
  printf 'say %s\n' "$marker" >&3
  for _ in $(seq 1 240); do
    grep -q "$marker" "$LOG" && break
    kill -0 "$ENGINE_PID" 2>/dev/null || { cat "$LOG"; return 1; }
    sleep 1
  done
  grep -q "$marker" "$LOG" || { echo "timed out locating $name"; return 1; }
  response=$(sed -n "$((line + 1)),\$p" "$LOG")
  if ! printf '%s\n' "$response" | grep -iq 'nearest'; then
    echo "no located start for $name in $dimension"
    printf '%s\n' "$response"
    return 1
  fi
  echo "located $name in $dimension"
}

locate minecraft:overworld lich
locate minecraft:the_end obsidilith
locate minecraft:the_nether gauntlet
locate minecraft:overworld void_blossom
printf 'stop\n' >&3
for _ in $(seq 1 30); do
  kill -0 "$ENGINE_PID" 2>/dev/null || break
  sleep 1
done
wait "$ENGINE_PID"
