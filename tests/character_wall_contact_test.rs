//! Character capsule pressed against vertical faces in the Mesa world
//! (gravity 10 x 20, JumpPower 50, WalkSpeed 16). A MoveTo held into a face
//! used to wedge the controller: the capsule hung on the face mid-air, perched
//! on a ledge's top edge, and a jump pressed into the face gave no rise.
use std::path::PathBuf;

use clawblox::game::instance::GameInstance;
use clawblox::game::script_bundle::load_world_and_entry_script;
use serde_json::json;
use uuid::Uuid;

struct Probe {
    instance: GameInstance,
    user_id: u64,
    hrp_id: u64,
}

impl Probe {
    fn new() -> Self {
        let world_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("worlds/mesa-island");
        let (_, source) = load_world_and_entry_script(&world_dir).unwrap();
        let mut instance = GameInstance::new(Uuid::new_v4(), None);
        instance.load_script(&source);
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "WallProbe"));
        let user_id = instance.players[&agent_id];
        let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();
        let mut probe = Self {
            instance,
            user_id,
            hrp_id,
        };
        probe.ticks(10);
        probe
    }

    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.instance.tick();
        }
    }

    fn pos(&self) -> [f32; 3] {
        self.instance
            .physics
            .get_character_position(self.hrp_id)
            .unwrap()
    }

    fn input(&mut self, input: &str, data: serde_json::Value) {
        self.instance
            .queue_agent_input(self.user_id, input.to_string(), data);
    }

    /// Stand at `start`, then walk into a face until pressed against it.
    fn press(&mut self, start: [f32; 3], into: [f32; 3]) -> [f32; 3] {
        self.instance
            .physics
            .set_character_position(self.hrp_id, start);
        self.ticks(60);
        self.input("MoveTo", json!({ "position": into }));
        self.ticks(120);
        self.pos()
    }
}

// The Spire's west face (x = -32, top at y = 20): too tall to climb.
const SPIRE_START: [f32; 3] = [-37.0, 4.0, -33.0];
const SPIRE_INTO: [f32; 3] = [-20.0, 3.0, -33.0];

#[test]
fn jump_pressed_into_wall_rises_along_face() {
    let mut p = Probe::new();
    let pressed = p.press(SPIRE_START, SPIRE_INTO);
    assert!(
        pressed[0] > -33.2,
        "should be pressed against the face: {pressed:?}"
    );

    p.input("Jump", json!({}));
    let mut max_y = pressed[1];
    for _ in 0..60 {
        p.ticks(1);
        max_y = max_y.max(p.pos()[1]);
    }
    assert!(
        max_y - pressed[1] > 6.0,
        "jump pressed into a wall should rise its full height; rose {:.3}",
        max_y - pressed[1]
    );
}

#[test]
fn character_pressed_into_wall_falls_instead_of_hanging() {
    let mut p = Probe::new();
    let pressed = p.press(SPIRE_START, SPIRE_INTO);
    let standing_y = pressed[1];

    // Stop, jump straight up beside the face, then press into it on the way
    // up: the capsule must come back down.
    p.input("Stop", json!({}));
    p.ticks(1);
    p.input("Jump", json!({}));
    p.ticks(15);
    p.input("MoveTo", json!({ "position": SPIRE_INTO }));
    p.ticks(120);
    let end = p.pos();
    assert!(
        end[1] < standing_y + 0.1,
        "character should fall along the face, not hang on it: standing y {standing_y:.3}, now {end:?}"
    );
}

#[test]
fn character_does_not_perch_on_ledge_lip() {
    // TallMesa's west face (x = -31, top at y = 6). Pressing into the face
    // with the capsule's lower hemisphere just above the top edge, the edge
    // contact is ~80° from up: steeper than walkable, so it must slide off.
    let mut p = Probe::new();
    let pressed = p.press([-36.0, 4.0, -18.0], [-20.0, 3.0, -18.0]);
    let standing_y = pressed[1];
    p.instance
        .physics
        .set_character_position(p.hrp_id, [pressed[0], 7.70, -18.0]);
    p.input("MoveTo", json!({ "position": [-20.0, 3.0, -18.0] }));
    p.ticks(120);
    let end = p.pos();
    assert!(
        end[1] < standing_y + 0.1 || end[0] > -30.0,
        "character should slide off the lip (or climb onto the top), not perch on it: {end:?}"
    );
}
