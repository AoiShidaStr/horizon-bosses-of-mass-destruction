#!/bin/bash
# Headless smoke test of Bosses of Mass Destruction on a scratch dedicated
# server (never /opt/minecraft): 127.0.0.1 only, offline mode, flat world, one
# survival bot (horizon-bots) next to each boss, commands over stdin.
S=/opt/build/bomd-smoke
SRV=$S/server
ENGINE=${ENGINE:-/opt/build/bomd-engine/horizon-engine}
pkill -f "[b]omd-smoke/resources" 2>/dev/null; pkill -f "[h]orizon-bots --server 127.0.0.1:25599" 2>/dev/null; sleep 1
rm -rf $SRV; mkdir -p $SRV/horizon-mods
cp /opt/build/bosses_of_mass_destruction.hmod $SRV/horizon-mods/
echo "mod.bosses_of_mass_destruction:true" > $SRV/horizon-mods.txt
echo "eula=true" > $SRV/eula.txt
cat > $SRV/server.properties <<P
server-ip=127.0.0.1
server-port=25599
online-mode=false
level-type=minecraft\:flat
gamemode=survival
force-gamemode=true
allow-flight=true
difficulty=normal
spawn-protection=0
pause-when-empty-seconds=0
view-distance=4
simulation-distance=4
generate-structures=false
spawn-monsters=false
spawn-animals=false
white-list=false
enforce-whitelist=false
P
: > $S/console.in
cmd() { echo "[smoke] > $*" >> $S/engine.log; echo "$*" >> $S/console.in; }
cd $SRV
export HORIZON_NO_AUDIO=1 BOMD_TRACE=1
(tail -f $S/console.in | $ENGINE --resources $S/resources --dedicated --nogui > $S/engine.log 2>&1; echo "[smoke] engine exited $?" >> $S/engine.log) &
for i in $(seq 1 240); do grep -q "Done (" $S/engine.log && break; grep -q "engine exited" $S/engine.log && exit 1; sleep 1; done
echo "[smoke] server up after ${i}s" >> $S/engine.log
/opt/build/target/debug/horizon-bots --server 127.0.0.1:25599 --bots 1 --behaviour idle --height 0 --ground --seconds 600 --name Bot --report $S/bots.json > $S/bots.log 2>&1 &
for i in $(seq 1 60); do grep -q "Bot0 joined" $S/engine.log && break; sleep 1; done
sleep 3
cmd "attribute Bot0 minecraft:max_health base set 1000"
cmd "effect give Bot0 minecraft:instant_health 1 10 true"
sleep 1
cmd "data get entity Bot0 Health"
cmd "data get entity Bot0 Pos"
for spec in obsidilith:300 lich:250 gauntlet:220 void_blossom:400; do
  b=${spec%%:*}; M=${spec##*:}; T="@e[type=bosses_of_mass_destruction:$b,limit=1]"
  echo "[smoke] ===== $b =====" >> $S/engine.log
  cmd "execute at Bot0 run summon bosses_of_mass_destruction:$b ~4 ~ ~"
  sleep 2
  cmd "data get entity $T Health"
  cmd "data get entity $T Pos"
  cmd "data merge entity $T {Fire:200s}"
  sleep 1
  cmd "data get entity $T Fire"
  cmd "data get entity $T Health"
  sleep 29
  cmd "data get entity Bot0 Health"
  cmd "data get entity $T Pos"
  cmd "data get entity $T Fire"
  cmd "damage $T $((M * 55 / 100)) minecraft:generic"
  sleep 1
  cmd "data get entity $T Health"
  cmd "damage $T 10 minecraft:generic"
  sleep 1
  cmd "data get entity $T Health"
  sleep 6
  cmd "damage $T $((M * 20 / 100)) minecraft:generic"
  sleep 1
  cmd "data get entity $T Health"
  sleep 15
  cmd "data get entity Bot0 Health"
  cmd "damage $T 2000 minecraft:generic"
  sleep 2
  cmd "execute if entity @e[type=bosses_of_mass_destruction:$b]"
  cmd "execute if entity @e[type=minecraft:item]"
  cmd "data get entity @e[type=minecraft:item,limit=1] Item"
  cmd "data get entity Bot0 Inventory"
  cmd "kill @e[type=minecraft:skeleton]"
  sleep 1
  cmd "kill @e[type=minecraft:item]"
  cmd "clear Bot0"
  cmd "effect give Bot0 minecraft:instant_health 1 10 true"
  sleep 2
done
cmd "list"
cmd "stop"
for i in $(seq 1 60); do grep -q "engine exited" $S/engine.log && break; sleep 1; done
pkill -f "[h]orizon-bots --server 127.0.0.1:25599"; pkill -f "[b]omd-smoke/resources"; pkill -f "[t]ail -f /opt/build/bomd-smoke/console.in"
echo "[smoke] DONE" >> $S/engine.log
