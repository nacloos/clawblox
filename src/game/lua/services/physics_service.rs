use mlua::{UserData, UserDataFields, UserDataMethods};

use crate::game::lua::instance::Instance;

/// Roblox-compatible PhysicsService surface API.
///
/// Clawblox currently simulates a single authoritative world without
/// Roblox-style per-group collision filtering, so collision-group methods are
/// accepted and treated as no-ops.
#[derive(Clone, Default)]
pub struct PhysicsService;

impl PhysicsService {
    pub fn new() -> Self {
        Self
    }
}

impl UserData for PhysicsService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, _| Ok("PhysicsService".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("PhysicsService".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("RegisterCollisionGroup", |_, _, _name: String| Ok(()));
        methods.add_method(
            "CollisionGroupSetCollidable",
            |_, _, (_group1, _group2, _collidable): (String, String, bool)| Ok(()),
        );
        methods.add_method(
            "SetPartCollisionGroup",
            |_, _, (_part, _group): (Instance, String)| Ok(()),
        );
    }
}
