//! The four bosses' models, built in code as a NeoForge mod's `createBodyLayer()`
//! builds its own (docs/native-mods.md, "Mobs drawn by a mod").
//!
//! The models are original geometry: boxes laid out for the original textures in
//! `assets/bosses_of_mass_destruction/textures/entity/`. Model coordinates are a
//! vanilla model's: y grows downwards, the front faces -z, one block is 16 units.
//! The feet of every model sit at y = 24, as the renderer's
//! `translate(0, -1.501, 0)` expects.
use horizon_client::mod_api::{
    CubeListBuilder, LayerDefinition, MeshDefinition, PartPose,
};

/// The texture size the models declare. The boxes are large (a boss is several
/// blocks tall), so their unwrapped faces need a 256x256 sheet; the 64x64
/// placeholder PNGs are stretched over it, which only scales the texels.
const SHEET: i32 = 256;

/// A whole SHEETxSHEET texture, handed out box by box so boxes never overlap.
struct Sheet {
    x: i32,
    y: i32,
    row: i32,
}

impl Sheet {
    const fn new() -> Self {
        Self { x: 0, y: 0, row: 0 }
    }

    /// The top-left texel of a box `dx` by `dy` by `dz` (vanilla's
    /// `(2*dz + 2*dx)` by `(dz + dy)` footprint).
    fn alloc(&mut self, dx: i32, dy: i32, dz: i32) -> (i32, i32) {
        let w = 2 * dz + 2 * dx;
        let h = dz + dy;
        if self.x + w > SHEET {
            self.x = 0;
            self.y += self.row;
            self.row = 0;
        }
        // Every model below fits in 256x256 (the tallest, the Obsidilith, ends
        // at row 82). A box past the edge would only sample the texture's wrap.
        let at = (self.x, self.y);
        self.x += w;
        self.row = self.row.max(h);
        at
    }
}

/// A box placed at its lowest corner (`origin`) with its `size`, using the next
/// free region of the sheet.
fn bx(sheet: &mut Sheet, origin: [f32; 3], size: [f32; 3]) -> CubeListBuilder {
    let (u, v) = sheet.alloc(size[0] as i32, size[1] as i32, size[2] as i32);
    CubeListBuilder::create().tex_offs(u, v).add_box(origin, size)
}

fn part(
    mesh: &mut MeshDefinition,
    parent: horizon_client::mod_api::PartId,
    name: &str,
    cubes: CubeListBuilder,
    pose: PartPose,
) -> horizon_client::mod_api::PartId {
    mesh.add_or_replace_child(parent, name, cubes, pose)
}

/// `bosses_of_mass_destruction:obsidilith#main`: a hunched obsidian giant with a
/// crystal core and two shoulder shards.
fn obsidilith() -> LayerDefinition {
    let mut mesh = MeshDefinition::new();
    let root = mesh.root();
    let mut sheet = Sheet::new();

    // Legs and hips (from the feet at y = 24 up to y = 4).
    let legs = bx(&mut sheet, [-11.0, 4.0, -8.0], [22.0, 20.0, 16.0]);
    let hips = part(&mut mesh, root, "hips", legs, PartPose::ZERO);

    let torso = bx(&mut sheet, [-13.0, -20.0, -10.0], [26.0, 24.0, 20.0]);
    let body = part(&mut mesh, hips, "body", torso, PartPose::ZERO);

    let head = bx(&mut sheet, [-7.0, -34.0, -7.0], [14.0, 14.0, 14.0]);
    part(&mut mesh, body, "head", head, PartPose::ZERO);

    let core = bx(&mut sheet, [-3.0, -17.0, -11.0], [6.0, 6.0, 2.0]);
    part(&mut mesh, body, "core", core, PartPose::ZERO);

    let shard = |sheet: &mut Sheet| bx(sheet, [-3.0, -5.0, -3.0], [6.0, 10.0, 6.0]);
    let left = shard(&mut sheet);
    part(&mut mesh, body, "left_shard", left, PartPose::offset(-18.0, -18.0, 0.0));
    let right = shard(&mut sheet);
    part(&mut mesh, body, "right_shard", right, PartPose::offset(18.0, -18.0, 0.0));

    let arm = |sheet: &mut Sheet| bx(sheet, [-4.0, 0.0, -4.0], [8.0, 30.0, 8.0]);
    let left_arm = arm(&mut sheet);
    part(&mut mesh, body, "left_arm", left_arm, PartPose::offset(-16.0, -14.0, 0.0));
    let right_arm = arm(&mut sheet);
    part(&mut mesh, body, "right_arm", right_arm, PartPose::offset(16.0, -14.0, 0.0));

    LayerDefinition::create(mesh, SHEET as u32, SHEET as u32)
}

/// `bosses_of_mass_destruction:lich#main`: a floating robed lich, crown, staff and
/// soul orb.
fn lich() -> LayerDefinition {
    let mut mesh = MeshDefinition::new();
    let root = mesh.root();
    let mut sheet = Sheet::new();

    let robe = bx(&mut sheet, [-9.0, -12.0, -8.0], [18.0, 36.0, 16.0]);
    let body = part(&mut mesh, root, "body", robe, PartPose::ZERO);

    let hood = bx(&mut sheet, [-6.0, -24.0, -6.0], [12.0, 12.0, 12.0]);
    let head = part(&mut mesh, body, "head", hood, PartPose::ZERO);

    let crown = bx(&mut sheet, [-8.0, -28.0, -8.0], [16.0, 4.0, 16.0]);
    part(&mut mesh, head, "crown", crown, PartPose::ZERO);

    let arm = |sheet: &mut Sheet| bx(sheet, [-3.0, 0.0, -3.0], [6.0, 26.0, 6.0]);
    let left_arm = arm(&mut sheet);
    part(&mut mesh, body, "left_arm", left_arm, PartPose::offset(-12.0, -10.0, 0.0));
    let right_arm = arm(&mut sheet);
    part(&mut mesh, body, "right_arm", right_arm, PartPose::offset(12.0, -10.0, 0.0));

    let staff = bx(&mut sheet, [-2.0, -20.0, -2.0], [3.0, 40.0, 3.0]);
    part(&mut mesh, body, "staff", staff, PartPose::offset(15.0, -8.0, 0.0));

    let orb = bx(&mut sheet, [-4.0, -4.0, -4.0], [8.0, 8.0, 8.0]);
    part(&mut mesh, body, "orb", orb, PartPose::offset(0.0, -34.0, 0.0));

    LayerDefinition::create(mesh, SHEET as u32, SHEET as u32)
}

/// `bosses_of_mass_destruction:gauntlet#main`: an armoured fist, wrist crystal and
/// four fingers that swing as one.
fn gauntlet() -> LayerDefinition {
    let mut mesh = MeshDefinition::new();
    let root = mesh.root();
    let mut sheet = Sheet::new();

    let wrist = bx(&mut sheet, [-7.0, 8.0, -7.0], [14.0, 16.0, 14.0]);
    let arm = part(&mut mesh, root, "wrist", wrist, PartPose::ZERO);

    let palm = bx(&mut sheet, [-11.0, -14.0, -7.0], [22.0, 22.0, 14.0]);
    let hand = part(&mut mesh, arm, "palm", palm, PartPose::ZERO);

    let gem = bx(&mut sheet, [-4.0, -5.0, -10.0], [8.0, 8.0, 4.0]);
    part(&mut mesh, arm, "gem", gem, PartPose::ZERO);

    let finger = |sheet: &mut Sheet| bx(sheet, [-2.0, -16.0, -5.0], [5.0, 16.0, 10.0]);
    for i in 0..4 {
        let index = finger(&mut sheet);
        part(
            &mut mesh,
            hand,
            &format!("finger_{i}"),
            index,
            PartPose::offset(-9.0 + i as f32 * 6.0, -14.0, 0.0),
        );
    }
    let thumb = bx(&mut sheet, [-4.0, -7.0, -6.0], [8.0, 14.0, 12.0]);
    part(&mut mesh, hand, "thumb", thumb, PartPose::offset(-15.0, -2.0, 0.0));

    LayerDefinition::create(mesh, SHEET as u32, SHEET as u32)
}

/// `bosses_of_mass_destruction:void_blossom#main`: a stem, six petals and a core
/// eye.
fn void_blossom() -> LayerDefinition {
    let mut mesh = MeshDefinition::new();
    let root = mesh.root();
    let mut sheet = Sheet::new();

    let stem = bx(&mut sheet, [-3.0, -6.0, -3.0], [6.0, 30.0, 6.0]);
    let body = part(&mut mesh, root, "stem", stem, PartPose::offset(0.0, 0.0, 0.0));

    let bulb = bx(&mut sheet, [-10.0, -22.0, -10.0], [20.0, 16.0, 20.0]);
    part(&mut mesh, body, "bulb", bulb, PartPose::offset(0.0, 0.0, 0.0));

    let petal = |sheet: &mut Sheet| bx(sheet, [-4.0, -16.0, -1.5], [8.0, 16.0, 3.0]);
    for i in 0..6 {
        let east = petal(&mut sheet);
        part(
            &mut mesh,
            body,
            &format!("petal_{i}"),
            east,
            PartPose::offset_and_rotation(
                0.0,
                -22.0,
                0.0,
                0.0,
                0.0,
                i as f32 * std::f32::consts::TAU / 6.0,
            ),
        );
    }

    let eye = bx(&mut sheet, [-4.0, -4.0, -4.0], [8.0, 8.0, 8.0]);
    part(&mut mesh, body, "eye", eye, PartPose::offset(0.0, -26.0, 0.0));

    LayerDefinition::create(mesh, SHEET as u32, SHEET as u32)
}

/// The layers this mod registers, by `<mod id>:<model>#main`.
pub fn layers() -> Vec<(String, LayerDefinition)> {
    vec![
        ("bosses_of_mass_destruction:obsidilith#main".to_owned(), obsidilith()),
        ("bosses_of_mass_destruction:lich#main".to_owned(), lich()),
        ("bosses_of_mass_destruction:gauntlet#main".to_owned(), gauntlet()),
        (
            "bosses_of_mass_destruction:void_blossom#main".to_owned(),
            void_blossom(),
        ),
    ]
}

/// The layer a boss's kind uses, if it is one of ours.
pub fn layer_for(kind: &str) -> Option<&'static str> {
    match kind {
        "bosses_of_mass_destruction:obsidilith" => {
            Some("bosses_of_mass_destruction:obsidilith#main")
        }
        "bosses_of_mass_destruction:lich" => Some("bosses_of_mass_destruction:lich#main"),
        "bosses_of_mass_destruction:gauntlet" => Some("bosses_of_mass_destruction:gauntlet#main"),
        "bosses_of_mass_destruction:void_blossom" => {
            Some("bosses_of_mass_destruction:void_blossom#main")
        }
        _ => None,
    }
}

/// The texture a boss's kind uses.
pub fn texture_for(kind: &str) -> Option<&'static str> {
    match kind {
        "bosses_of_mass_destruction:obsidilith" => {
            Some("bosses_of_mass_destruction:entity/obsidilith")
        }
        "bosses_of_mass_destruction:lich" => Some("bosses_of_mass_destruction:entity/lich"),
        "bosses_of_mass_destruction:gauntlet" => Some("bosses_of_mass_destruction:entity/gauntlet"),
        "bosses_of_mass_destruction:void_blossom" => {
            Some("bosses_of_mass_destruction:entity/void_blossom")
        }
        _ => None,
    }
}
