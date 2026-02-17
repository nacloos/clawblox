use mlua::{UserData, UserDataFields, UserDataMethods};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::game::lua::instance::Instance;

/// Roblox-compatible PhysicsService surface API.
///
/// Clawblox tracks collision groups and pairwise collidable flags. Part
/// assignments are applied through `SetPartCollisionGroup`, and the runtime
/// consults these flags for script-level touch event gating. Full solver-level
/// collision filtering parity is still pending.
#[derive(Default)]
struct PhysicsServiceData {
    registered_groups: HashSet<String>,
    explicit_pair_collidable: HashMap<(String, String), bool>,
}

#[derive(Clone, Default)]
pub struct PhysicsService {
    data: Arc<Mutex<PhysicsServiceData>>,
}

impl PhysicsService {
    pub fn new() -> Self {
        let mut data = PhysicsServiceData::default();
        data.registered_groups.insert("Default".to_string());
        Self {
            data: Arc::new(Mutex::new(data)),
        }
    }

    fn canonical_pair(group1: &str, group2: &str) -> (String, String) {
        if group1 <= group2 {
            (group1.to_string(), group2.to_string())
        } else {
            (group2.to_string(), group1.to_string())
        }
    }

    fn ensure_registered_group(&self, group: &str) {
        self.data
            .lock()
            .unwrap()
            .registered_groups
            .insert(group.to_string());
    }

    pub fn groups_collidable(&self, group1: &str, group2: &str) -> bool {
        let data = self.data.lock().unwrap();
        let key = Self::canonical_pair(group1, group2);
        data.explicit_pair_collidable
            .get(&key)
            .copied()
            .unwrap_or(true)
    }
}

impl UserData for PhysicsService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, _| Ok("PhysicsService".to_string()));
        fields.add_field_method_get("ClassName", |_, _| Ok("PhysicsService".to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("RegisterCollisionGroup", |_, this, name: String| {
            this.ensure_registered_group(&name);
            Ok(())
        });
        methods.add_method(
            "CollisionGroupSetCollidable",
            |_, this, (group1, group2, collidable): (String, String, bool)| {
                this.ensure_registered_group(&group1);
                this.ensure_registered_group(&group2);
                let key = PhysicsService::canonical_pair(&group1, &group2);
                this.data
                    .lock()
                    .unwrap()
                    .explicit_pair_collidable
                    .insert(key, collidable);
                Ok(())
            },
        );
        methods.add_method(
            "SetPartCollisionGroup",
            |_, this, (part, group): (Instance, String)| {
                this.ensure_registered_group(&group);
                let mut data = part.data.lock().unwrap();
                if let Some(part_data) = &mut data.part_data {
                    part_data.collision_group = group;
                }
                Ok(())
            },
        );
    }
}
