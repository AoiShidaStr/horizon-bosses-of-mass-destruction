#!/bin/bash
# Load every boss shrine on an isolated flat dedicated server, check its altar
# and chest, and roll the guaranteed offering. Never uses the live server.
set -euo pipefail

S=/opt/build/bomd-smoke
SRV=$S/server-structures
ENGINE=${ENGINE:-/opt/build/bomd-engine/horizon-engine}
LOG=$S/structures-engine.log
IN=$S/structures-console.in
PORT=25632

rm -rf "$SRV"
mkdir -p "$SRV/horizon-mods"
cp /opt/build/bosses_of_mass_destruction.hmod "$SRV/horizon-mods/"
printf 'mod.bosses_of_mass_destruction:true\n' > "$SRV/horizon-mods.txt"
printf 'eula=true\n' > "$SRV/eula.txt"
cat > "$SRV/server.properties" <<P
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-type=minecraft\:flat
gamemode=creative
difficulty=normal
spawn-protection=0
pause-when-empty-seconds=0
view-distance=4
simulation-distance=4
generate-structures=false
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

cmd() { printf '%s\n' "$1" >&3; sleep 1; }
check() { cmd "execute if block $1 $2 $3 $4"; }
specs=(
  "lich|0|0|4|4|2|4|minecraft:wither_skeleton_skull"
  "obsidilith|32|0|7|7|4|7|minecraft:ender_eye"
  "gauntlet|64|0|5|5|2|5|minecraft:netherite_scrap"
  "void_blossom|96|0|6|6|4|6|minecraft:spore_blossom"
)

for spec in "${specs[@]}"; do
  IFS='|' read -r name ox oz ax az cx cz offering <<< "$spec"
  structure=bosses_of_mass_destruction:${name}_shrine
  chest=bosses_of_mass_destruction:chests/${name}_shrine
  cmd "forceload add $ox $oz"
  sleep 2
  cmd "place structure $structure $ox -60 $oz"
  check "$((ox + ax))" -60 "$((oz + az))" minecraft:crying_obsidian
  check "$((ox + cx))" -59 "$((oz + cz))" minecraft:chest
  cmd "data get block $((ox + cx)) -59 $((oz + cz))"
  cmd "loot spawn $((ox + 2)) -59 $((oz + 2)) loot $chest"
  sleep 1
  cmd "execute if entity @e[type=minecraft:item,distance=..16,nbt={Item:{id:\"$offering\"}}]"
  cmd "kill @e[type=minecraft:item]"
done

cmd "stop"
for _ in $(seq 1 30); do
  kill -0 "$ENGINE_PID" 2>/dev/null || break
  sleep 1
done
wait "$ENGINE_PID"
if grep -qE 'Test failed|Unknown structure|No such structure|Failed to load' "$LOG"; then
  grep -E 'Test failed|Unknown structure|No such structure|Failed to load' "$LOG"
  exit 1
fi
tests=$(grep -c 'Test passed' "$LOG" || true)
if [ "$tests" -lt 8 ]; then
  echo "expected 8 shrine altar/chest/offering checks, found $tests"
  cat "$LOG"
  exit 1
fi
echo "Boss shrine smoke passed: $tests command assertions"
