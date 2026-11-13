#!/bin/bash
# Headless test of the altar (an offering used on crying obsidian wakes a boss)
# on a scratch dedicated server (never /opt/minecraft): 127.0.0.1 only,
# offline mode, flat world. For each boss a survival bot (horizon-bots
# --click) right-clicks the altar with two offerings in its hand; the boss
# must wake once and one offering must be used up. A second bot then clicks
# again: refused while the boss is awake.
S=/opt/build/bomd-smoke
SRV=$S/server-altar
LOG=$S/altar-engine.log
IN=$S/altar-console.in
ENGINE=${ENGINE:-/opt/build/bomd-engine/horizon-engine}
BOTS=/opt/build/target/debug/horizon-bots
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
difficulty=normal
spawn-protection=0
pause-when-empty-seconds=0
view-distance=4
simulation-distance=4
generate-structures=false
spawn-monsters=false
spawn-animals=false
white-list=false
P
: > $IN
cmd() { echo "[altar] > $*" >> $LOG; echo "$*" >> $IN; }
cd $SRV
export HORIZON_NO_AUDIO=1 BOMD_TRACE=1
(tail -f $IN | $ENGINE --resources $S/resources --dedicated --nogui > $LOG 2>&1; echo "[altar] engine exited $?" >> $LOG) &
for i in $(seq 1 240); do grep -q "Done (" $LOG && break; grep -q "engine exited" $LOG && exit 1; sleep 1; done
echo "[altar] server up after ${i}s" >> $LOG
ALTAR="3 -61 0"
# Players join at the world spawn itself (no random spread), so each bot
# stands within reach of the altar; the altar's chunk stays loaded.
cmd "gamerule respawn_radius 0"
cmd "forceload add -16 -16 16 16"
sleep 2
# A bot that right-clicks the altar's top 3 s after joining, holding $2 x2.
click() {
  local name=$1 item=$2
  $BOTS --server 127.0.0.1:25599 --bots 1 --behaviour idle --height 0 --ground --seconds 12 \
    --name $name --click 3,-61,0:0:0:0 >> $S/altar-bots.log 2>&1 &
  for i in $(seq 1 60); do grep -q "${name}0 joined" $LOG && break; sleep 0.25; done
  cmd "item replace entity ${name}0 weapon.mainhand with $item 2"
  sleep 7
  cmd "data get entity ${name}0 SelectedItem"
}
cmd "setblock $ALTAR minecraft:crying_obsidian"
n=0
for spec in obsidilith:minecraft:ender_eye lich:minecraft:wither_skeleton_skull gauntlet:minecraft:netherite_scrap void_blossom:minecraft:spore_blossom; do
  b=${spec%%:*}; item=${spec#*:}; n=$((n+1))
  echo "[altar] ===== $b ($item) =====" >> $LOG
  click A$n $item
  cmd "execute if entity @e[type=bosses_of_mass_destruction:$b]"
  cmd "data get entity @e[type=bosses_of_mass_destruction:$b,limit=1] Pos"
  sleep 3
  echo "[altar] ----- second call while awake" >> $LOG
  click B$n $item
  cmd "execute if entity @e[type=bosses_of_mass_destruction:$b]"
  sleep 1
  cmd "kill @e[type=bosses_of_mass_destruction:$b]"
  cmd "kill @e[type=minecraft:skeleton]"
  sleep 2
  cmd "kill @e[type=minecraft:item]"
  cmd "setblock $ALTAR minecraft:crying_obsidian"
  sleep 1
done
echo "[altar] ----- an eye of ender on plain stone wakes nothing" >> $LOG
cmd "setblock $ALTAR minecraft:stone"
click C1 minecraft:ender_eye
cmd "execute if entity @e[type=bosses_of_mass_destruction:obsidilith]"
cmd "list"
cmd "stop"
for i in $(seq 1 60); do grep -q "engine exited" $LOG && break; sleep 1; done
pkill -f "[h]orizon-bots --server 127.0.0.1:25599"; pkill -f "[b]omd-smoke/resources"; pkill -f "[t]ail -f $IN"
echo "[altar] DONE" >> $LOG
