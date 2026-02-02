use mlua::{FromLua, Lua, Result, UserData, UserDataFields, UserDataMethods, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use super::events::{create_signal, RBXScriptSignal};
use super::services::WorkspaceService;
use super::types::{CFrame, Color3, Material, PartType, Vector3};

static INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

pub type InstanceRef = Arc<Mutex<InstanceData>>;
pub type WeakInstanceRef = Weak<Mutex<InstanceData>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId(pub u64);

impl InstanceId {
    pub fn new() -> Self {
        Self(INSTANCE_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassName {
    Instance,
    BasePart,
    Part,
    Model,
    Humanoid,
    Player,
    Folder,
    Workspace,
    Players,
    RunService,
    Camera,
}

impl ClassName {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClassName::Instance => "Instance",
            ClassName::BasePart => "BasePart",
            ClassName::Part => "Part",
            ClassName::Model => "Model",
            ClassName::Humanoid => "Humanoid",
            ClassName::Player => "Player",
            ClassName::Folder => "Folder",
            ClassName::Workspace => "Workspace",
            ClassName::Players => "Players",
            ClassName::RunService => "RunService",
            ClassName::Camera => "Camera",
        }
    }

    pub fn is_a(&self, class_name: &str) -> bool {
        match class_name {
            "Instance" => true,
            "BasePart" => matches!(self, ClassName::BasePart | ClassName::Part),
            "Part" => matches!(self, ClassName::Part),
            "Model" => matches!(self, ClassName::Model),
            "Humanoid" => matches!(self, ClassName::Humanoid),
            "Player" => matches!(self, ClassName::Player),
            "Folder" => matches!(self, ClassName::Folder),
            "Workspace" => matches!(self, ClassName::Workspace),
            "Players" => matches!(self, ClassName::Players),
            "RunService" => matches!(self, ClassName::RunService),
            "Camera" => matches!(self, ClassName::Camera),
            _ => self.as_str() == class_name,
        }
    }
}

pub struct InstanceData {
    pub id: InstanceId,
    pub name: String,
    pub class_name: ClassName,
    pub parent: Option<WeakInstanceRef>,
    pub children: Vec<InstanceRef>,
    pub attributes: HashMap<String, AttributeValue>,

    pub child_added: RBXScriptSignal,
    pub child_removed: RBXScriptSignal,
    pub destroying: RBXScriptSignal,
    pub attribute_changed: RBXScriptSignal,

    pub part_data: Option<PartData>,
    pub humanoid_data: Option<HumanoidData>,
    pub player_data: Option<PlayerData>,
    pub model_data: Option<ModelData>,

    destroyed: bool,
}

#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Bool(bool),
    Vector3(Vector3),
    Color3(Color3),
    Nil,
}

impl AttributeValue {
    /// Convert to JSON value for API responses
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            AttributeValue::String(s) => serde_json::Value::String(s.clone()),
            AttributeValue::Number(n) => serde_json::json!(*n),
            AttributeValue::Bool(b) => serde_json::Value::Bool(*b),
            AttributeValue::Vector3(v) => serde_json::json!([v.x, v.y, v.z]),
            AttributeValue::Color3(c) => serde_json::json!([c.r, c.g, c.b]),
            AttributeValue::Nil => serde_json::Value::Null,
        }
    }
}

/// Convert a HashMap of AttributeValue to JSON-serializable HashMap
pub fn attributes_to_json(
    attrs: &std::collections::HashMap<String, AttributeValue>,
) -> std::collections::HashMap<String, serde_json::Value> {
    attrs.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()
}

#[derive(Debug, Clone)]
pub struct PartData {
    pub position: Vector3,
    pub cframe: CFrame,
    pub size: Vector3,
    pub anchored: bool,
    pub can_collide: bool,
    pub can_touch: bool,
    pub transparency: f32,
    pub color: Color3,
    pub material: Material,
    pub velocity: Vector3,
    pub shape: PartType,

    pub touched: RBXScriptSignal,
    pub touch_ended: RBXScriptSignal,
}

impl Default for PartData {
    fn default() -> Self {
        Self {
            position: Vector3::zero(),
            cframe: CFrame::identity(),
            size: Vector3::new(4.0, 1.0, 2.0),
            anchored: false,
            can_collide: true,
            can_touch: true,
            transparency: 0.0,
            color: Color3::new(0.6, 0.6, 0.6),
            material: Material::Plastic,
            velocity: Vector3::zero(),
            shape: PartType::Block,
            touched: create_signal("Touched"),
            touch_ended: create_signal("TouchEnded"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HumanoidData {
    pub health: f32,
    pub max_health: f32,
    pub walk_speed: f32,
    pub jump_power: f32,
    pub jump_height: f32,
    pub auto_rotate: bool,
    pub hip_height: f32,
    /// Movement target set by MoveTo()
    pub move_to_target: Option<Vector3>,

    pub died: RBXScriptSignal,
    pub health_changed: RBXScriptSignal,
    pub move_to_finished: RBXScriptSignal,
}

impl Default for HumanoidData {
    fn default() -> Self {
        Self {
            health: 100.0,
            max_health: 100.0,
            walk_speed: 16.0,
            jump_power: 50.0,
            jump_height: 7.2,
            auto_rotate: true,
            hip_height: 2.0,
            move_to_target: None,
            died: create_signal("Died"),
            health_changed: create_signal("HealthChanged"),
            move_to_finished: create_signal("MoveToFinished"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerData {
    pub user_id: u64,
    pub display_name: String,
    pub character: Option<WeakInstanceRef>,

    pub character_added: RBXScriptSignal,
    pub character_removing: RBXScriptSignal,
}

impl PlayerData {
    pub fn new(user_id: u64, name: &str) -> Self {
        Self {
            user_id,
            display_name: name.to_string(),
            character: None,
            character_added: create_signal("CharacterAdded"),
            character_removing: create_signal("CharacterRemoving"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelData {
    pub primary_part: Option<WeakInstanceRef>,
}

impl Default for ModelData {
    fn default() -> Self {
        Self { primary_part: None }
    }
}

impl InstanceData {
    pub fn new(class_name: ClassName, name: &str) -> Self {
        Self {
            id: InstanceId::new(),
            name: name.to_string(),
            class_name,
            parent: None,
            children: Vec::new(),
            attributes: HashMap::new(),
            child_added: create_signal("ChildAdded"),
            child_removed: create_signal("ChildRemoved"),
            destroying: create_signal("Destroying"),
            attribute_changed: create_signal("AttributeChanged"),
            part_data: None,
            humanoid_data: None,
            player_data: None,
            model_data: None,
            destroyed: false,
        }
    }

    pub fn new_part(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Part, name);
        inst.part_data = Some(PartData::default());
        inst
    }

    pub fn new_model(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Model, name);
        inst.model_data = Some(ModelData::default());
        inst
    }

    pub fn new_humanoid(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Humanoid, name);
        inst.humanoid_data = Some(HumanoidData::default());
        inst
    }

    pub fn new_player(user_id: u64, name: &str) -> Self {
        let mut inst = Self::new(ClassName::Player, name);
        inst.player_data = Some(PlayerData::new(user_id, name));
        inst
    }

    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }
}

#[derive(Clone)]
pub struct Instance {
    pub data: InstanceRef,
}

impl Instance {
    pub fn new(class_name: ClassName, name: &str) -> Self {
        Self {
            data: Arc::new(Mutex::new(InstanceData::new(class_name, name))),
        }
    }

    pub fn from_data(data: InstanceData) -> Self {
        Self {
            data: Arc::new(Mutex::new(data)),
        }
    }

    pub fn from_ref(data: InstanceRef) -> Self {
        Self { data }
    }

    pub fn id(&self) -> InstanceId {
        self.data.lock().unwrap().id
    }

    pub fn name(&self) -> String {
        self.data.lock().unwrap().name.clone()
    }

    pub fn set_name(&self, name: &str) {
        self.data.lock().unwrap().name = name.to_string();
    }

    pub fn class_name(&self) -> ClassName {
        self.data.lock().unwrap().class_name
    }

    pub fn is_a(&self, class_name: &str) -> bool {
        self.data.lock().unwrap().class_name.is_a(class_name)
    }

    pub fn parent(&self) -> Option<Instance> {
        self.data
            .lock()
            .unwrap()
            .parent
            .as_ref()
            .and_then(|w| w.upgrade())
            .map(Instance::from_ref)
    }

    pub fn set_parent(&self, parent: Option<&Instance>) {
        if let Some(old_parent) = self.parent() {
            let my_id = self.id();
            old_parent
                .data
                .lock()
                .unwrap()
                .children
                .retain(|c| c.lock().unwrap().id != my_id);
        }

        if let Some(new_parent) = parent {
            self.data.lock().unwrap().parent = Some(Arc::downgrade(&new_parent.data));
            new_parent
                .data
                .lock()
                .unwrap()
                .children
                .push(Arc::clone(&self.data));
        } else {
            self.data.lock().unwrap().parent = None;
        }
    }

    pub fn get_children(&self) -> Vec<Instance> {
        self.data
            .lock()
            .unwrap()
            .children
            .iter()
            .map(|c| Instance::from_ref(Arc::clone(c)))
            .collect()
    }

    pub fn get_descendants(&self) -> Vec<Instance> {
        let mut result = Vec::new();
        for child in self.get_children() {
            result.push(child.clone());
            result.extend(child.get_descendants());
        }
        result
    }

    pub fn find_first_child(&self, name: &str, recursive: bool) -> Option<Instance> {
        for child in self.get_children() {
            if child.name() == name {
                return Some(child);
            }
            if recursive {
                if let Some(found) = child.find_first_child(name, true) {
                    return Some(found);
                }
            }
        }
        None
    }

    pub fn find_first_child_of_class(&self, class_name: &str) -> Option<Instance> {
        for child in self.get_children() {
            if child.is_a(class_name) {
                return Some(child);
            }
        }
        None
    }

    pub fn is_descendant_of(&self, ancestor: &Instance) -> bool {
        let mut current = self.parent();
        while let Some(p) = current {
            if p.id() == ancestor.id() {
                return true;
            }
            current = p.parent();
        }
        false
    }

    pub fn destroy(&self, lua: &Lua) -> Result<()> {
        {
            let mut data = self.data.lock().unwrap();
            if data.destroyed {
                return Ok(());
            }
            data.destroyed = true;
        }

        let destroying = self.data.lock().unwrap().destroying.clone();
        destroying.fire(lua, mlua::MultiValue::new())?;

        for child in self.get_children() {
            child.destroy(lua)?;
        }

        self.set_parent(None);
        Ok(())
    }

    pub fn clone_instance(&self) -> Instance {
        let data = self.data.lock().unwrap();
        let mut new_data = InstanceData::new(data.class_name, &data.name);
        new_data.attributes = data.attributes.clone();
        new_data.part_data = data.part_data.clone();
        new_data.humanoid_data = data.humanoid_data.clone();
        new_data.model_data = data.model_data.clone();
        drop(data);

        let new_instance = Instance::from_data(new_data);

        for child in self.get_children() {
            let cloned_child = child.clone_instance();
            cloned_child.set_parent(Some(&new_instance));
        }

        new_instance
    }

    pub fn set_attribute(&self, name: &str, value: AttributeValue) {
        self.data
            .lock()
            .unwrap()
            .attributes
            .insert(name.to_string(), value);
    }

    pub fn get_attribute(&self, name: &str) -> Option<AttributeValue> {
        self.data.lock().unwrap().attributes.get(name).cloned()
    }

    pub fn get_attributes(&self) -> HashMap<String, AttributeValue> {
        self.data.lock().unwrap().attributes.clone()
    }

    pub fn weak_ref(&self) -> WeakInstanceRef {
        Arc::downgrade(&self.data)
    }
}

impl FromLua for Instance {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Instance>().map(|v| v.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Instance".to_string(),
                message: Some("expected Instance".to_string()),
            }),
        }
    }
}

impl UserData for Instance {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, this| Ok(this.name()));
        fields.add_field_method_set("Name", |_, this, name: String| {
            this.set_name(&name);
            Ok(())
        });

        fields.add_field_method_get("ClassName", |_, this| {
            Ok(this.class_name().as_str().to_string())
        });

        fields.add_field_method_get("Parent", |_, this| Ok(this.parent()));
        fields.add_field_method_set("Parent", |_, this, parent: Value| {
            match parent {
                Value::Nil => {
                    this.set_parent(None);
                }
                Value::UserData(ud) => {
                    if let Ok(inst) = ud.borrow::<Instance>() {
                        this.set_parent(Some(&inst));
                    } else if let Ok(ws) = ud.borrow::<WorkspaceService>() {
                        this.set_parent(Some(&ws.instance));
                    } else {
                        return Err(mlua::Error::runtime("Parent must be an Instance or nil"));
                    }
                }
                _ => return Err(mlua::Error::runtime("Parent must be an Instance or nil")),
            }
            Ok(())
        });

        fields.add_field_method_get("ChildAdded", |_, this| {
            Ok(this.data.lock().unwrap().child_added.clone())
        });
        fields.add_field_method_get("ChildRemoved", |_, this| {
            Ok(this.data.lock().unwrap().child_removed.clone())
        });
        fields.add_field_method_get("Destroying", |_, this| {
            Ok(this.data.lock().unwrap().destroying.clone())
        });
        fields.add_field_method_get("AttributeChanged", |_, this| {
            Ok(this.data.lock().unwrap().attribute_changed.clone())
        });

        fields.add_field_method_get("Position", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.position))
        });
        fields.add_field_method_set("Position", |_, this, pos: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.position = pos;
                part.cframe.position = pos;
            }
            Ok(())
        });

        fields.add_field_method_get("CFrame", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.cframe))
        });
        fields.add_field_method_set("CFrame", |_, this, cf: CFrame| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.cframe = cf;
                part.position = cf.position;
            }
            Ok(())
        });

        fields.add_field_method_get("Size", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.size))
        });
        fields.add_field_method_set("Size", |_, this, size: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.size = size;
            }
            Ok(())
        });

        fields.add_field_method_get("Anchored", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.anchored))
        });
        fields.add_field_method_set("Anchored", |_, this, anchored: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.anchored = anchored;
            }
            Ok(())
        });

        fields.add_field_method_get("CanCollide", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.can_collide))
        });
        fields.add_field_method_set("CanCollide", |_, this, can_collide: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.can_collide = can_collide;
            }
            Ok(())
        });

        fields.add_field_method_get("CanTouch", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.can_touch))
        });
        fields.add_field_method_set("CanTouch", |_, this, can_touch: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.can_touch = can_touch;
            }
            Ok(())
        });

        fields.add_field_method_get("Transparency", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.transparency))
        });
        fields.add_field_method_set("Transparency", |_, this, transparency: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.transparency = transparency.clamp(0.0, 1.0);
            }
            Ok(())
        });

        fields.add_field_method_get("Color", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.color))
        });
        fields.add_field_method_set("Color", |_, this, color: Color3| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.color = color;
            }
            Ok(())
        });

        fields.add_field_method_get("Material", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.material))
        });
        fields.add_field_method_set("Material", |_, this, material: Material| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.material = material;
            }
            Ok(())
        });

        fields.add_field_method_get("Velocity", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.velocity))
        });
        fields.add_field_method_set("Velocity", |_, this, velocity: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.velocity = velocity;
            }
            Ok(())
        });

        fields.add_field_method_get("AssemblyLinearVelocity", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.velocity))
        });

        fields.add_field_method_get("Shape", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.shape))
        });
        fields.add_field_method_set("Shape", |_, this, shape: PartType| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.shape = shape;
            }
            Ok(())
        });

        fields.add_field_method_get("Touched", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.touched.clone()))
        });

        fields.add_field_method_get("TouchEnded", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.touch_ended.clone()))
        });

        fields.add_field_method_get("Health", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.health))
        });
        fields.add_field_method_set("Health", |_, this, health: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.health = health.max(0.0).min(humanoid.max_health);
            }
            Ok(())
        });

        fields.add_field_method_get("MaxHealth", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.max_health))
        });
        fields.add_field_method_set("MaxHealth", |_, this, max_health: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.max_health = max_health.max(0.0);
            }
            Ok(())
        });

        fields.add_field_method_get("WalkSpeed", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.walk_speed))
        });
        fields.add_field_method_set("WalkSpeed", |_, this, walk_speed: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.walk_speed = walk_speed.max(0.0);
            }
            Ok(())
        });

        fields.add_field_method_get("JumpPower", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.jump_power))
        });
        fields.add_field_method_set("JumpPower", |_, this, jump_power: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.jump_power = jump_power.max(0.0);
            }
            Ok(())
        });

        fields.add_field_method_get("JumpHeight", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.jump_height))
        });
        fields.add_field_method_set("JumpHeight", |_, this, jump_height: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.jump_height = jump_height.max(0.0);
            }
            Ok(())
        });

        fields.add_field_method_get("AutoRotate", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.auto_rotate))
        });
        fields.add_field_method_set("AutoRotate", |_, this, auto_rotate: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.auto_rotate = auto_rotate;
            }
            Ok(())
        });

        fields.add_field_method_get("HipHeight", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.hip_height))
        });
        fields.add_field_method_set("HipHeight", |_, this, hip_height: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.hip_height = hip_height;
            }
            Ok(())
        });

        fields.add_field_method_get("Died", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.died.clone()))
        });

        fields.add_field_method_get("HealthChanged", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.health_changed.clone()))
        });

        fields.add_field_method_get("MoveToFinished", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .humanoid_data
                .as_ref()
                .map(|h| h.move_to_finished.clone()))
        });

        fields.add_field_method_get("UserId", |_, this| {
            let data = this.data.lock().unwrap();
            // Return as f64 to ensure it's a Lua number type (not Integer) for table key compatibility
            Ok(data.player_data.as_ref().map(|p| p.user_id as f64))
        });

        fields.add_field_method_get("DisplayName", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.player_data.as_ref().map(|p| p.display_name.clone()))
        });

        fields.add_field_method_get("Character", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .player_data
                .as_ref()
                .and_then(|p| p.character.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });

        fields.add_field_method_get("CharacterAdded", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .player_data
                .as_ref()
                .map(|p| p.character_added.clone()))
        });

        fields.add_field_method_get("CharacterRemoving", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .player_data
                .as_ref()
                .map(|p| p.character_removing.clone()))
        });

        fields.add_field_method_get("PrimaryPart", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .model_data
                .as_ref()
                .and_then(|m| m.primary_part.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });
        fields.add_field_method_set("PrimaryPart", |_, this, part: Option<Instance>| {
            let mut data = this.data.lock().unwrap();
            if let Some(model) = &mut data.model_data {
                model.primary_part = part.map(|p| Arc::downgrade(&p.data));
            }
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Clone", |_, this, ()| Ok(this.clone_instance()));

        methods.add_method("Destroy", |lua, this, ()| this.destroy(lua));

        methods.add_method(
            "FindFirstChild",
            |_, this, (name, recursive): (String, Option<bool>)| {
                Ok(this.find_first_child(&name, recursive.unwrap_or(false)))
            },
        );

        methods.add_method("FindFirstChildOfClass", |_, this, class_name: String| {
            Ok(this.find_first_child_of_class(&class_name))
        });

        methods.add_method("GetChildren", |_, this, ()| Ok(this.get_children()));

        methods.add_method("GetDescendants", |_, this, ()| Ok(this.get_descendants()));

        methods.add_method("IsA", |_, this, class_name: String| {
            Ok(this.is_a(&class_name))
        });

        methods.add_method("IsDescendantOf", |_, this, ancestor: Instance| {
            Ok(this.is_descendant_of(&ancestor))
        });

        methods.add_method("SetAttribute", |_, this, (name, value): (String, Value)| {
            let attr_value = match value {
                Value::Nil => AttributeValue::Nil,
                Value::Boolean(b) => AttributeValue::Bool(b),
                Value::Integer(n) => AttributeValue::Number(n as f64),
                Value::Number(n) => AttributeValue::Number(n),
                Value::String(s) => AttributeValue::String(s.to_str()?.to_string()),
                Value::UserData(ud) => {
                    if let Ok(v) = ud.borrow::<Vector3>() {
                        AttributeValue::Vector3(*v)
                    } else if let Ok(c) = ud.borrow::<Color3>() {
                        AttributeValue::Color3(*c)
                    } else {
                        return Err(mlua::Error::runtime("Unsupported attribute type"));
                    }
                }
                _ => return Err(mlua::Error::runtime("Unsupported attribute type")),
            };
            this.set_attribute(&name, attr_value);
            Ok(())
        });

        methods.add_method("GetAttribute", |lua, this, name: String| {
            match this.get_attribute(&name) {
                Some(AttributeValue::Nil) => Ok(Value::Nil),
                Some(AttributeValue::Bool(b)) => Ok(Value::Boolean(b)),
                Some(AttributeValue::Number(n)) => Ok(Value::Number(n)),
                Some(AttributeValue::String(s)) => Ok(Value::String(lua.create_string(&s)?)),
                Some(AttributeValue::Vector3(v)) => Ok(Value::UserData(lua.create_userdata(v)?)),
                Some(AttributeValue::Color3(c)) => Ok(Value::UserData(lua.create_userdata(c)?)),
                None => Ok(Value::Nil),
            }
        });

        methods.add_method("GetAttributes", |lua, this, ()| {
            let table = lua.create_table()?;
            for (key, value) in this.get_attributes() {
                let lua_value = match value {
                    AttributeValue::Nil => Value::Nil,
                    AttributeValue::Bool(b) => Value::Boolean(b),
                    AttributeValue::Number(n) => Value::Number(n),
                    AttributeValue::String(s) => Value::String(lua.create_string(&s)?),
                    AttributeValue::Vector3(v) => Value::UserData(lua.create_userdata(v)?),
                    AttributeValue::Color3(c) => Value::UserData(lua.create_userdata(c)?),
                };
                table.set(key, lua_value)?;
            }
            Ok(table)
        });

        methods.add_method("TakeDamage", |lua, this, amount: f32| {
            let (old_health, new_health, health_changed, died) = {
                let mut data = this.data.lock().unwrap();
                if let Some(humanoid) = &mut data.humanoid_data {
                    let old = humanoid.health;
                    humanoid.health = (humanoid.health - amount).max(0.0);
                    (
                        old,
                        humanoid.health,
                        humanoid.health_changed.clone(),
                        humanoid.died.clone(),
                    )
                } else {
                    return Ok(());
                }
            };

            if old_health != new_health {
                health_changed.fire(
                    lua,
                    mlua::MultiValue::from_iter([Value::Number(new_health as f64)]),
                )?;
                if new_health <= 0.0 && old_health > 0.0 {
                    died.fire(lua, mlua::MultiValue::new())?;
                }
            }
            Ok(())
        });

        methods.add_method(
            "Move",
            |_, _this, (_direction, _relative): (Vector3, Option<bool>)| Ok(()),
        );

        methods.add_method(
            "MoveTo",
            |_, this, (position, _part): (Vector3, Option<Instance>)| {
                let mut data = this.data.lock().unwrap();
                if let Some(humanoid) = &mut data.humanoid_data {
                    humanoid.move_to_target = Some(position);
                }
                Ok(())
            },
        );

        methods.add_method("GetPrimaryPartCFrame", |_, this, ()| {
            let data = this.data.lock().unwrap();
            if let Some(model) = &data.model_data {
                if let Some(primary) = &model.primary_part {
                    if let Some(primary_ref) = primary.upgrade() {
                        let primary_data = primary_ref.lock().unwrap();
                        if let Some(part) = &primary_data.part_data {
                            return Ok(Some(part.cframe));
                        }
                    }
                }
            }
            Ok(None)
        });

        methods.add_method("SetPrimaryPartCFrame", |_, this, cframe: CFrame| {
            let primary_cframe = {
                let data = this.data.lock().unwrap();
                if let Some(model) = &data.model_data {
                    if let Some(primary) = &model.primary_part {
                        if let Some(primary_ref) = primary.upgrade() {
                            let primary_data = primary_ref.lock().unwrap();
                            primary_data.part_data.as_ref().map(|p| p.cframe)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(old_cframe) = primary_cframe {
                let offset = old_cframe.inverse().multiply(&cframe);

                for descendant in this.get_descendants() {
                    let mut data = descendant.data.lock().unwrap();
                    if let Some(part) = &mut data.part_data {
                        part.cframe = part.cframe.multiply(&offset);
                        part.position = part.cframe.position;
                    }
                }
            }
            Ok(())
        });

        methods.add_method("LoadCharacter", |_, _this, ()| Ok(()));

        methods.add_method("Kick", |_, _this, _message: Option<String>| Ok(()));

        methods.add_meta_method(mlua::MetaMethod::Index, |_, this, key: String| {
            Ok(this.find_first_child(&key, false))
        });

        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| Ok(this.name()));

        methods.add_meta_method(mlua::MetaMethod::Eq, |_, this, other: Instance| {
            Ok(this.id() == other.id())
        });
    }
}

pub fn register_instance(lua: &Lua) -> Result<()> {
    let instance_table = lua.create_table()?;

    instance_table.set(
        "new",
        lua.create_function(
            |_, (class_name, parent): (String, Option<Instance>)| -> Result<Instance> {
                let instance = match class_name.as_str() {
                    "Part" => Instance::from_data(InstanceData::new_part("Part")),
                    "Model" => Instance::from_data(InstanceData::new_model("Model")),
                    "Humanoid" => Instance::from_data(InstanceData::new_humanoid("Humanoid")),
                    "Folder" => Instance::new(ClassName::Folder, "Folder"),
                    _ => Instance::new(ClassName::Instance, &class_name),
                };

                if let Some(parent) = parent {
                    instance.set_parent(Some(&parent));
                }

                Ok(instance)
            },
        )?,
    )?;

    lua.globals().set("Instance", instance_table)?;

    Ok(())
}
