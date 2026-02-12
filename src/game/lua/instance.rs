use mlua::{
    AnyUserData, FromLua, Lua, MultiValue, Result, Table, UserData, UserDataFields,
    UserDataMethods, Value,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use super::events::{create_signal, track_yielded_threads, RBXScriptSignal};
use super::runtime::Game;
use super::services::{PlayersService, WorkspaceService};
use super::types::{
    CFrame, Color3, HumanoidStateType, Material, PartType, PhysicalProperties, UDim, UDim2,
    Vector3,
};
use crate::game::constants::humanoid as humanoid_consts;

mod class_name;
pub use class_name::ClassName;

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

pub struct InstanceData {
    pub id: InstanceId,
    pub name: String,
    pub class_name: ClassName,
    pub parent: Option<WeakInstanceRef>,
    pub children: Vec<InstanceRef>,
    pub attributes: HashMap<String, AttributeValue>,
    pub tags: HashSet<String>,

    pub child_added: RBXScriptSignal,
    pub child_removed: RBXScriptSignal,
    pub ancestry_changed: RBXScriptSignal,
    pub destroying: RBXScriptSignal,
    pub attribute_changed: RBXScriptSignal,

    pub part_data: Option<PartData>,
    pub humanoid_data: Option<HumanoidData>,
    pub player_data: Option<PlayerData>,
    pub animation_data: Option<AnimationData>,
    pub animator_data: Option<AnimatorData>,
    pub model_data: Option<ModelData>,
    pub gui_data: Option<GuiObjectData>,
    pub weld_data: Option<WeldData>,
    pub billboard_gui_data: Option<BillboardGuiData>,
    pub ui_corner_data: Option<UICornerData>,
    pub script_data: Option<ScriptData>,
    pub int_value_data: Option<IntValueData>,
    pub number_value_data: Option<NumberValueData>,
    pub bool_value_data: Option<BoolValueData>,
    pub string_value_data: Option<StringValueData>,
    pub object_value_data: Option<ObjectValueData>,
    pub remote_event_data: Option<RemoteEventData>,
    pub body_velocity_data: Option<BodyVelocityData>,
    pub body_force_data: Option<BodyForceData>,
    pub body_angular_velocity_data: Option<BodyAngularVelocityData>,
    pub attachment_data: Option<AttachmentData>,
    pub align_orientation_data: Option<AlignOrientationData>,
    pub property_changed_signals: HashMap<String, RBXScriptSignal>,
    pub attribute_changed_signals: HashMap<String, RBXScriptSignal>,

    destroyed: bool,
}

#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Bool(bool),
    Vector3(Vector3),
    Color3(Color3),
    Table(Vec<AttributeValue>),
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
            AttributeValue::Table(items) => {
                serde_json::Value::Array(items.iter().map(|v| v.to_json()).collect())
            }
            AttributeValue::Nil => serde_json::Value::Null,
        }
    }
}

/// Convert a HashMap of AttributeValue to JSON-serializable HashMap
pub fn attributes_to_json(
    attrs: &std::collections::HashMap<String, AttributeValue>,
) -> std::collections::HashMap<String, serde_json::Value> {
    attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.to_json()))
        .collect()
}

pub use super::animation::{
    default_animation_length_seconds, AnimationScheduler, AnimationTrack, AnimationTrackSnapshot,
};

#[derive(Debug, Clone)]
pub struct PartData {
    pub position: Vector3,
    pub cframe: CFrame,
    pub size: Vector3,
    pub anchored: bool,
    pub can_collide: bool,
    pub can_query: bool,
    pub collision_group: String,
    pub can_touch: bool,
    pub transparency: f32,
    pub color: Color3,
    pub material: Material,
    pub top_surface: String,
    pub bottom_surface: String,
    pub velocity: Vector3,
    pub custom_physical_properties: Option<PhysicalProperties>,
    pub shape: PartType,
    pub position_dirty: bool,
    pub size_dirty: bool,
    pub anchored_dirty: bool,
    pub can_collide_dirty: bool,
    pub velocity_dirty: bool,
    pub shape_dirty: bool,

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
            can_query: true,
            collision_group: "Default".to_string(),
            can_touch: true,
            transparency: 0.0,
            color: Color3::new(0.6, 0.6, 0.6),
            material: Material::Plastic,
            top_surface: "Smooth".to_string(),
            bottom_surface: "Smooth".to_string(),
            velocity: Vector3::zero(),
            custom_physical_properties: None,
            shape: PartType::Block,
            position_dirty: false,
            size_dirty: false,
            anchored_dirty: false,
            can_collide_dirty: false,
            velocity_dirty: false,
            shape_dirty: false,
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
    pub health_display_distance: f32,
    pub name_display_distance: f32,
    pub hip_height: f32,
    pub move_direction: Vector3,
    /// Input movement direction set by Humanoid:Move().
    pub move_input_direction: Vector3,
    pub running_speed: f32,
    pub state: HumanoidStateType,
    /// One-shot jump request consumed by game movement update
    pub jump_requested: bool,
    /// Movement target set by MoveTo()
    pub move_to_target: Option<Vector3>,
    /// Flag to cancel movement (set by CancelMoveTo)
    pub cancel_move_to: bool,

    pub died: RBXScriptSignal,
    pub health_changed: RBXScriptSignal,
    pub running: RBXScriptSignal,
    pub state_changed: RBXScriptSignal,
    pub move_to_finished: RBXScriptSignal,
}

impl Default for HumanoidData {
    fn default() -> Self {
        Self {
            health: humanoid_consts::DEFAULT_HEALTH,
            max_health: humanoid_consts::DEFAULT_MAX_HEALTH,
            walk_speed: humanoid_consts::DEFAULT_WALK_SPEED,
            jump_power: humanoid_consts::DEFAULT_JUMP_POWER,
            jump_height: humanoid_consts::DEFAULT_JUMP_HEIGHT,
            auto_rotate: true,
            health_display_distance: 100.0,
            name_display_distance: 100.0,
            hip_height: humanoid_consts::DEFAULT_HIP_HEIGHT,
            move_direction: Vector3::zero(),
            move_input_direction: Vector3::zero(),
            running_speed: 0.0,
            state: HumanoidStateType::None,
            jump_requested: false,
            move_to_target: None,
            cancel_move_to: false,
            died: create_signal("Died"),
            health_changed: create_signal("HealthChanged"),
            running: create_signal("Running"),
            state_changed: create_signal("StateChanged"),
            move_to_finished: create_signal("MoveToFinished"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerData {
    pub user_id: u64,
    pub display_name: String,
    pub character: Option<WeakInstanceRef>,
    pub player_gui: Option<WeakInstanceRef>,

    pub character_added: RBXScriptSignal,
    pub character_removing: RBXScriptSignal,
}

impl PlayerData {
    pub fn new(user_id: u64, name: &str) -> Self {
        Self {
            user_id,
            display_name: name.to_string(),
            character: None,
            player_gui: None,
            character_added: create_signal("CharacterAdded"),
            character_removing: create_signal("CharacterRemoving"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelData {
    pub primary_part: Option<WeakInstanceRef>,
}

#[derive(Debug, Clone)]
pub struct AnimationData {
    pub animation_id: String,
}

impl Default for AnimationData {
    fn default() -> Self {
        Self {
            animation_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnimatorData {
    pub loaded_tracks: u32,
}

impl Default for ModelData {
    fn default() -> Self {
        Self { primary_part: None }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptData {
    pub source: String,
    pub disabled: bool,
}

impl Default for ScriptData {
    fn default() -> Self {
        Self {
            source: String::new(),
            disabled: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntValueData {
    pub value: i64,
    pub changed: RBXScriptSignal,
}

impl Default for IntValueData {
    fn default() -> Self {
        Self {
            value: 0,
            changed: create_signal("Changed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NumberValueData {
    pub value: f64,
    pub changed: RBXScriptSignal,
}

impl Default for NumberValueData {
    fn default() -> Self {
        Self {
            value: 0.0,
            changed: create_signal("Changed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoolValueData {
    pub value: bool,
    pub changed: RBXScriptSignal,
}

impl Default for BoolValueData {
    fn default() -> Self {
        Self {
            value: false,
            changed: create_signal("Changed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StringValueData {
    pub value: String,
    pub changed: RBXScriptSignal,
}

impl Default for StringValueData {
    fn default() -> Self {
        Self {
            value: String::new(),
            changed: create_signal("Changed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectValueData {
    pub value: Option<WeakInstanceRef>,
    pub changed: RBXScriptSignal,
}

impl Default for ObjectValueData {
    fn default() -> Self {
        Self {
            value: None,
            changed: create_signal("Changed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteEventData {
    pub on_server_event: RBXScriptSignal,
}

#[derive(Debug, Clone)]
pub struct BodyVelocityData {
    pub velocity: Vector3,
    pub max_force: Vector3,
}

impl Default for BodyVelocityData {
    fn default() -> Self {
        Self {
            velocity: Vector3::zero(),
            max_force: Vector3::zero(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BodyForceData {
    pub force: Vector3,
}

impl Default for BodyForceData {
    fn default() -> Self {
        Self {
            force: Vector3::zero(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BodyAngularVelocityData {
    pub angular_velocity: Vector3,
    pub max_torque: Vector3,
}

impl Default for BodyAngularVelocityData {
    fn default() -> Self {
        Self {
            angular_velocity: Vector3::zero(),
            max_torque: Vector3::zero(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AttachmentData {}

#[derive(Debug, Clone)]
pub struct AlignOrientationData {
    pub attachment0: Option<WeakInstanceRef>,
    pub attachment1: Option<WeakInstanceRef>,
    pub responsiveness: f32,
    pub rigidity_enabled: bool,
}

impl Default for AlignOrientationData {
    fn default() -> Self {
        Self {
            attachment0: None,
            attachment1: None,
            responsiveness: 10.0,
            rigidity_enabled: false,
        }
    }
}

impl Default for RemoteEventData {
    fn default() -> Self {
        Self {
            on_server_event: create_signal("OnServerEvent"),
        }
    }
}

/// Data for Weld constraints
#[derive(Debug, Clone)]
pub struct WeldData {
    pub part0: Option<WeakInstanceRef>,
    pub part1: Option<WeakInstanceRef>,
    pub c0: CFrame,
    pub c1: CFrame,
    pub enabled: bool,
}

impl Default for WeldData {
    fn default() -> Self {
        Self {
            part0: None,
            part1: None,
            c0: CFrame::identity(),
            c1: CFrame::identity(),
            enabled: true,
        }
    }
}

/// Data for BillboardGui (3D GUI that floats above parts)
#[derive(Debug, Clone)]
pub struct BillboardGuiData {
    pub size: UDim2,
    pub studs_offset: Vector3,
    pub always_on_top: bool,
    pub enabled: bool,
    pub adornee: Option<WeakInstanceRef>,
}

impl Default for BillboardGuiData {
    fn default() -> Self {
        Self {
            size: UDim2::new(0.0, 100, 0.0, 50),
            studs_offset: Vector3::new(0.0, 0.0, 0.0),
            always_on_top: false,
            enabled: true,
            adornee: None,
        }
    }
}

/// Data for UICorner
#[derive(Debug, Clone, Copy)]
pub struct UICornerData {
    pub corner_radius: UDim,
}

impl Default for UICornerData {
    fn default() -> Self {
        Self {
            corner_radius: UDim::new(0.0, 0),
        }
    }
}

/// Data for GUI objects (Frame, TextLabel, TextButton, etc.)
#[derive(Debug, Clone)]
pub struct GuiObjectData {
    // Layout properties
    pub position: UDim2,
    pub size: UDim2,
    pub anchor_point: (f32, f32), // Vector2 equivalent: 0-1 for X and Y
    pub rotation: f32,
    pub z_index: i32,
    pub layout_order: i32,
    pub visible: bool,

    // Appearance
    pub background_color: Color3,
    pub background_transparency: f32,
    pub border_color: Color3,
    pub border_size_pixel: i32,

    // Text properties (for TextLabel, TextButton)
    pub text: Option<String>,
    pub text_color: Option<Color3>,
    pub text_size: Option<f32>,
    pub text_transparency: Option<f32>,
    pub text_stroke_transparency: Option<f32>,
    pub font: Option<String>,
    pub text_scaled: bool,
    pub text_x_alignment: TextXAlignment,
    pub text_y_alignment: TextYAlignment,

    // Image properties (for ImageLabel, ImageButton)
    pub image: Option<String>,
    pub image_color: Option<Color3>,
    pub image_transparency: Option<f32>,

    // ScreenGui-specific
    pub display_order: i32,
    pub ignore_gui_inset: bool,
    pub enabled: bool,
    pub reset_on_spawn: bool,

    // Events (for GuiButton types)
    pub mouse_button1_click: Option<RBXScriptSignal>,
    pub mouse_button1_down: Option<RBXScriptSignal>,
    pub mouse_button1_up: Option<RBXScriptSignal>,
    pub mouse_enter: Option<RBXScriptSignal>,
    pub mouse_leave: Option<RBXScriptSignal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextXAlignment {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextYAlignment {
    Top,
    #[default]
    Center,
    Bottom,
}

impl Default for GuiObjectData {
    fn default() -> Self {
        Self {
            position: UDim2::new(0.0, 0, 0.0, 0),
            size: UDim2::new(0.0, 100, 0.0, 100),
            anchor_point: (0.0, 0.0),
            rotation: 0.0,
            z_index: 1,
            layout_order: 0,
            visible: true,
            background_color: Color3::new(1.0, 1.0, 1.0),
            background_transparency: 0.0,
            border_color: Color3::new(0.1, 0.1, 0.1),
            border_size_pixel: 1,
            text: None,
            text_color: None,
            text_size: None,
            text_transparency: None,
            text_stroke_transparency: None,
            font: None,
            text_scaled: false,
            text_x_alignment: TextXAlignment::default(),
            text_y_alignment: TextYAlignment::default(),
            image: None,
            image_color: None,
            image_transparency: None,
            display_order: 0,
            ignore_gui_inset: false,
            enabled: true,
            reset_on_spawn: true,
            mouse_button1_click: None,
            mouse_button1_down: None,
            mouse_button1_up: None,
            mouse_enter: None,
            mouse_leave: None,
        }
    }
}

impl GuiObjectData {
    /// Create data for a Frame
    pub fn new_frame() -> Self {
        Self::default()
    }

    /// Create data for a TextLabel
    pub fn new_text_label() -> Self {
        Self {
            text: Some(String::new()),
            text_color: Some(Color3::new(0.0, 0.0, 0.0)),
            text_size: Some(14.0),
            text_transparency: Some(0.0),
            text_stroke_transparency: Some(1.0),
            font: Some("SourceSans".to_string()),
            background_transparency: 1.0, // TextLabels default to transparent background
            ..Self::default()
        }
    }

    /// Create data for a TextButton
    pub fn new_text_button() -> Self {
        Self {
            text: Some(String::new()),
            text_color: Some(Color3::new(0.0, 0.0, 0.0)),
            text_size: Some(14.0),
            text_transparency: Some(0.0),
            text_stroke_transparency: Some(1.0),
            font: Some("SourceSans".to_string()),
            mouse_button1_click: Some(create_signal("MouseButton1Click")),
            mouse_button1_down: Some(create_signal("MouseButton1Down")),
            mouse_button1_up: Some(create_signal("MouseButton1Up")),
            mouse_enter: Some(create_signal("MouseEnter")),
            mouse_leave: Some(create_signal("MouseLeave")),
            ..Self::default()
        }
    }

    /// Create data for an ImageLabel
    pub fn new_image_label() -> Self {
        Self {
            image: Some(String::new()),
            image_color: Some(Color3::new(1.0, 1.0, 1.0)),
            image_transparency: Some(0.0),
            background_transparency: 1.0,
            ..Self::default()
        }
    }

    /// Create data for an ImageButton
    pub fn new_image_button() -> Self {
        Self {
            image: Some(String::new()),
            image_color: Some(Color3::new(1.0, 1.0, 1.0)),
            image_transparency: Some(0.0),
            mouse_button1_click: Some(create_signal("MouseButton1Click")),
            mouse_button1_down: Some(create_signal("MouseButton1Down")),
            mouse_button1_up: Some(create_signal("MouseButton1Up")),
            mouse_enter: Some(create_signal("MouseEnter")),
            mouse_leave: Some(create_signal("MouseLeave")),
            ..Self::default()
        }
    }

    /// Create data for a ScreenGui
    pub fn new_screen_gui() -> Self {
        Self {
            enabled: true,
            display_order: 0,
            ignore_gui_inset: false,
            reset_on_spawn: true,
            background_transparency: 1.0, // ScreenGui is fully transparent
            ..Self::default()
        }
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
            tags: HashSet::new(),
            child_added: create_signal("ChildAdded"),
            child_removed: create_signal("ChildRemoved"),
            ancestry_changed: create_signal("AncestryChanged"),
            destroying: create_signal("Destroying"),
            attribute_changed: create_signal("AttributeChanged"),
            part_data: None,
            humanoid_data: None,
            player_data: None,
            animation_data: None,
            animator_data: None,
            model_data: None,
            gui_data: None,
            weld_data: None,
            billboard_gui_data: None,
            ui_corner_data: None,
            script_data: None,
            int_value_data: None,
            number_value_data: None,
            bool_value_data: None,
            string_value_data: None,
            object_value_data: None,
            remote_event_data: None,
            body_velocity_data: None,
            body_force_data: None,
            body_angular_velocity_data: None,
            attachment_data: None,
            align_orientation_data: None,
            property_changed_signals: HashMap::new(),
            attribute_changed_signals: HashMap::new(),
            destroyed: false,
        }
    }

    pub fn new_weld(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Weld, name);
        inst.weld_data = Some(WeldData::default());
        inst
    }

    pub fn new_billboard_gui(name: &str) -> Self {
        let mut inst = Self::new(ClassName::BillboardGui, name);
        inst.billboard_gui_data = Some(BillboardGuiData::default());
        inst
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

    pub fn new_animation(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Animation, name);
        inst.animation_data = Some(AnimationData::default());
        inst
    }

    pub fn new_animator(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Animator, name);
        inst.animator_data = Some(AnimatorData::default());
        inst
    }

    pub fn new_script(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Script, name);
        inst.script_data = Some(ScriptData::default());
        inst
    }

    pub fn new_module_script(name: &str) -> Self {
        let mut inst = Self::new(ClassName::ModuleScript, name);
        inst.script_data = Some(ScriptData::default());
        inst
    }

    pub fn new_server_script_service() -> Self {
        Self::new(ClassName::ServerScriptService, "ServerScriptService")
    }

    pub fn new_replicated_storage() -> Self {
        Self::new(ClassName::ReplicatedStorage, "ReplicatedStorage")
    }

    pub fn new_remote_event(name: &str) -> Self {
        let mut inst = Self::new(ClassName::RemoteEvent, name);
        inst.remote_event_data = Some(RemoteEventData::default());
        inst
    }

    pub fn new_int_value(name: &str) -> Self {
        let mut inst = Self::new(ClassName::IntValue, name);
        inst.int_value_data = Some(IntValueData::default());
        inst
    }

    pub fn new_number_value(name: &str) -> Self {
        let mut inst = Self::new(ClassName::NumberValue, name);
        inst.number_value_data = Some(NumberValueData::default());
        inst
    }

    pub fn new_bool_value(name: &str) -> Self {
        let mut inst = Self::new(ClassName::BoolValue, name);
        inst.bool_value_data = Some(BoolValueData::default());
        inst
    }

    pub fn new_string_value(name: &str) -> Self {
        let mut inst = Self::new(ClassName::StringValue, name);
        inst.string_value_data = Some(StringValueData::default());
        inst
    }

    pub fn new_object_value(name: &str) -> Self {
        let mut inst = Self::new(ClassName::ObjectValue, name);
        inst.object_value_data = Some(ObjectValueData::default());
        inst
    }

    pub fn new_body_velocity(name: &str) -> Self {
        let mut inst = Self::new(ClassName::BodyVelocity, name);
        inst.body_velocity_data = Some(BodyVelocityData::default());
        inst
    }

    pub fn new_body_force(name: &str) -> Self {
        let mut inst = Self::new(ClassName::BodyForce, name);
        inst.body_force_data = Some(BodyForceData::default());
        inst
    }

    pub fn new_body_angular_velocity(name: &str) -> Self {
        let mut inst = Self::new(ClassName::BodyAngularVelocity, name);
        inst.body_angular_velocity_data = Some(BodyAngularVelocityData::default());
        inst
    }

    pub fn new_attachment(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Attachment, name);
        inst.attachment_data = Some(AttachmentData::default());
        inst
    }

    pub fn new_align_orientation(name: &str) -> Self {
        let mut inst = Self::new(ClassName::AlignOrientation, name);
        inst.align_orientation_data = Some(AlignOrientationData::default());
        inst
    }

    pub fn new_player_gui(name: &str) -> Self {
        Self::new(ClassName::PlayerGui, name)
    }

    pub fn new_screen_gui(name: &str) -> Self {
        let mut inst = Self::new(ClassName::ScreenGui, name);
        inst.gui_data = Some(GuiObjectData::new_screen_gui());
        inst
    }

    pub fn new_uicorner(name: &str) -> Self {
        let mut inst = Self::new(ClassName::UICorner, name);
        inst.ui_corner_data = Some(UICornerData::default());
        inst
    }

    pub fn new_frame(name: &str) -> Self {
        let mut inst = Self::new(ClassName::Frame, name);
        inst.gui_data = Some(GuiObjectData::new_frame());
        inst
    }

    pub fn new_text_label(name: &str) -> Self {
        let mut inst = Self::new(ClassName::TextLabel, name);
        inst.gui_data = Some(GuiObjectData::new_text_label());
        inst
    }

    pub fn new_text_button(name: &str) -> Self {
        let mut inst = Self::new(ClassName::TextButton, name);
        inst.gui_data = Some(GuiObjectData::new_text_button());
        inst
    }

    pub fn new_image_label(name: &str) -> Self {
        let mut inst = Self::new(ClassName::ImageLabel, name);
        inst.gui_data = Some(GuiObjectData::new_image_label());
        inst
    }

    pub fn new_image_button(name: &str) -> Self {
        let mut inst = Self::new(ClassName::ImageButton, name);
        inst.gui_data = Some(GuiObjectData::new_image_button());
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
    pub(crate) fn cached_userdata(lua: &Lua, instance: &Instance) -> Result<AnyUserData> {
        const CACHE_KEY: &str = "__clawblox_instance_userdata_cache";
        let cache = match lua.named_registry_value::<Table>(CACHE_KEY) {
            Ok(table) => table,
            Err(_) => {
                let table = lua.create_table()?;
                lua.set_named_registry_value(CACHE_KEY, table.clone())?;
                table
            }
        };

        let id_key = instance.id().0.to_string();
        if let Ok(existing) = cache.get::<AnyUserData>(id_key.as_str()) {
            return Ok(existing);
        }

        let created = lua.create_userdata(instance.clone())?;
        cache.set(id_key.as_str(), created.clone())?;
        Ok(created)
    }

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

    pub fn fire_remote_on_server_event(
        &self,
        lua: &Lua,
        player: &Instance,
        args: Vec<Value>,
    ) -> Result<()> {
        let signal = {
            let data = self.data.lock().unwrap();
            data.remote_event_data
                .as_ref()
                .map(|r| r.on_server_event.clone())
        };
        let Some(signal) = signal else {
            return Ok(());
        };

        let mut values = Vec::with_capacity(args.len() + 1);
        values.push(Value::UserData(Instance::cached_userdata(lua, player)?));
        values.extend(args);
        let threads = signal.fire_as_coroutines(lua, MultiValue::from_iter(values))?;
        crate::game::lua::events::track_yielded_threads(lua, threads)?;
        Ok(())
    }

    fn fire_signal(lua: &Lua, signal: &RBXScriptSignal, args: MultiValue) -> Result<()> {
        let threads = signal.fire_as_coroutines(lua, args)?;
        crate::game::lua::events::track_yielded_threads(lua, threads)?;
        Ok(())
    }

    fn fire_property_changed_if_listening(&self, lua: &Lua, property_name: &str) -> Result<()> {
        let signal = self
            .data
            .lock()
            .unwrap()
            .property_changed_signals
            .get(property_name)
            .cloned();
        if let Some(signal) = signal {
            Self::fire_signal(lua, &signal, MultiValue::new())?;
        }
        Ok(())
    }

    fn ensure_animator_child_for_humanoid(&self) -> Instance {
        if let Some(existing) = self
            .get_children()
            .into_iter()
            .find(|child| child.class_name() == ClassName::Animator)
        {
            return existing;
        }

        let animator = Instance::from_data(InstanceData::new_animator("Animator"));
        animator.set_parent(Some(self));
        animator
    }

    fn load_animation_track_for_animator(
        &self,
        lua: &Lua,
        animation: &Instance,
    ) -> Result<AnimationTrack> {
        let animation_id = {
            let data = animation.data.lock().unwrap();
            if data.class_name != ClassName::Animation {
                return Err(mlua::Error::runtime(
                    "LoadAnimation expects an Animation instance",
                ));
            }
            data.animation_data
                .as_ref()
                .map(|a| a.animation_id.clone())
                .unwrap_or_default()
        };

        if animation_id.is_empty() {
            return Err(mlua::Error::runtime("AnimationId is empty"));
        }

        let length = default_animation_length_seconds(&animation_id);
        let track = AnimationTrack::new(animation_id, self.id().0, length);

        if let Some(scheduler) = lua.app_data_ref::<AnimationScheduler>() {
            scheduler.register(track.clone());
        }

        {
            let mut data = self.data.lock().unwrap();
            if let Some(animator_data) = &mut data.animator_data {
                animator_data.loaded_tracks += 1;
            }
        }

        Ok(track)
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

            // BodyVelocity compatibility shim: parenting under a Part applies current velocity.
            let body_velocity = {
                let data = self.data.lock().unwrap();
                data.body_velocity_data.clone()
            };
            if let Some(bv) = body_velocity {
                let mut parent_data = new_parent.data.lock().unwrap();
                if let Some(part) = &mut parent_data.part_data {
                    part.velocity = bv.velocity;
                    part.velocity_dirty = true;
                }
            }
        } else {
            self.data.lock().unwrap().parent = None;
        }
    }

    pub fn set_parent_with_lua(&self, parent: Option<&Instance>, lua: &Lua) -> Result<()> {
        let old_parent = self.parent();
        self.set_parent(parent);

        if let Some(old_parent) = old_parent {
            let signal = old_parent.data.lock().unwrap().child_removed.clone();
            let child_ud = lua.create_userdata(self.clone())?;
            Self::fire_signal(
                lua,
                &signal,
                MultiValue::from_iter([Value::UserData(child_ud)]),
            )?;
        }

        if let Some(new_parent) = parent {
            let signal = new_parent.data.lock().unwrap().child_added.clone();
            let child_ud = lua.create_userdata(self.clone())?;
            Self::fire_signal(
                lua,
                &signal,
                MultiValue::from_iter([Value::UserData(child_ud)]),
            )?;
        }

        self.fire_ancestry_changed_recursive(lua)?;
        self.fire_property_changed_if_listening(lua, "Parent")?;
        Ok(())
    }

    fn fire_ancestry_changed_recursive(&self, lua: &Lua) -> Result<()> {
        let ancestry_signal = self.data.lock().unwrap().ancestry_changed.clone();
        let this_ud = lua.create_userdata(self.clone())?;
        let parent_value = match self.parent() {
            Some(parent) => Value::UserData(lua.create_userdata(parent)?),
            None => Value::Nil,
        };
        Self::fire_signal(
            lua,
            &ancestry_signal,
            MultiValue::from_iter([Value::UserData(this_ud), parent_value]),
        )?;

        for child in self.get_children() {
            child.fire_ancestry_changed_recursive(lua)?;
        }

        Ok(())
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
        let threads = destroying.fire_as_coroutines(lua, mlua::MultiValue::new())?;
        crate::game::lua::events::track_yielded_threads(lua, threads)?;

        for child in self.get_children() {
            child.destroy(lua)?;
        }

        self.set_parent_with_lua(None, lua)?;
        Ok(())
    }

    pub fn clone_instance(&self) -> Instance {
        let data = self.data.lock().unwrap();
        let mut new_data = InstanceData::new(data.class_name, &data.name);
        new_data.attributes = data.attributes.clone();
        new_data.tags = data.tags.clone();
        new_data.part_data = data.part_data.clone();
        new_data.humanoid_data = data.humanoid_data.clone();
        new_data.model_data = data.model_data.clone();
        if let Some(int_data) = &data.int_value_data {
            new_data.int_value_data = Some(IntValueData {
                value: int_data.value,
                changed: create_signal("Changed"),
            });
        }
        if let Some(number_data) = &data.number_value_data {
            new_data.number_value_data = Some(NumberValueData {
                value: number_data.value,
                changed: create_signal("Changed"),
            });
        }
        if let Some(bool_data) = &data.bool_value_data {
            new_data.bool_value_data = Some(BoolValueData {
                value: bool_data.value,
                changed: create_signal("Changed"),
            });
        }
        if let Some(string_data) = &data.string_value_data {
            new_data.string_value_data = Some(StringValueData {
                value: string_data.value.clone(),
                changed: create_signal("Changed"),
            });
        }
        if let Some(object_data) = &data.object_value_data {
            new_data.object_value_data = Some(ObjectValueData {
                value: object_data.value.clone(),
                changed: create_signal("Changed"),
            });
        }
        if data.remote_event_data.is_some() {
            new_data.remote_event_data = Some(RemoteEventData::default());
        }
        new_data.body_velocity_data = data.body_velocity_data.clone();
        new_data.body_force_data = data.body_force_data;
        new_data.body_angular_velocity_data = data.body_angular_velocity_data;
        new_data.attachment_data = data.attachment_data.clone();
        new_data.align_orientation_data = data.align_orientation_data.clone();

        // Clone GUI data but create fresh signals to avoid sharing handlers
        if let Some(gui) = &data.gui_data {
            let mut cloned_gui = gui.clone();
            // Create new signals for the cloned instance
            if cloned_gui.mouse_button1_click.is_some() {
                cloned_gui.mouse_button1_click = Some(create_signal("MouseButton1Click"));
                cloned_gui.mouse_button1_down = Some(create_signal("MouseButton1Down"));
                cloned_gui.mouse_button1_up = Some(create_signal("MouseButton1Up"));
                cloned_gui.mouse_enter = Some(create_signal("MouseEnter"));
                cloned_gui.mouse_leave = Some(create_signal("MouseLeave"));
            }
            new_data.gui_data = Some(cloned_gui);
        }

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

    pub fn set_attribute_with_lua(
        &self,
        lua: &Lua,
        name: &str,
        value: AttributeValue,
    ) -> Result<()> {
        self.set_attribute(name, value);

        let (attribute_changed, specific_signal) = {
            let mut data = self.data.lock().unwrap();
            let specific = data
                .attribute_changed_signals
                .entry(name.to_string())
                .or_insert_with(|| create_signal(&format!("AttributeChanged_{}", name)))
                .clone();
            (data.attribute_changed.clone(), specific)
        };

        let attr_name = Value::String(lua.create_string(name)?);
        Self::fire_signal(lua, &attribute_changed, MultiValue::from_iter([attr_name]))?;
        Self::fire_signal(lua, &specific_signal, MultiValue::new())?;
        Ok(())
    }

    pub fn ensure_property_signal(&self, property_name: &str) -> RBXScriptSignal {
        let mut data = self.data.lock().unwrap();
        data.property_changed_signals
            .entry(property_name.to_string())
            .or_insert_with(|| create_signal(&format!("{}Changed", property_name)))
            .clone()
    }

    pub fn ensure_attribute_signal(&self, attr_name: &str) -> RBXScriptSignal {
        let mut data = self.data.lock().unwrap();
        data.attribute_changed_signals
            .entry(attr_name.to_string())
            .or_insert_with(|| create_signal(&format!("AttributeChanged_{}", attr_name)))
            .clone()
    }

    pub fn get_attribute(&self, name: &str) -> Option<AttributeValue> {
        self.data.lock().unwrap().attributes.get(name).cloned()
    }

    pub fn get_attributes(&self) -> HashMap<String, AttributeValue> {
        self.data.lock().unwrap().attributes.clone()
    }

    pub fn add_tag(&self, tag: &str) {
        self.data.lock().unwrap().tags.insert(tag.to_string());
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.data.lock().unwrap().tags.contains(tag)
    }

    pub fn remove_tag(&self, tag: &str) {
        self.data.lock().unwrap().tags.remove(tag);
    }

    pub fn get_tags(&self) -> HashSet<String> {
        self.data.lock().unwrap().tags.clone()
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

fn parent_instance_from_lua_value(parent: Value) -> Result<Option<Instance>> {
    match parent {
        Value::Nil => Ok(None),
        Value::UserData(ud) => {
            if let Ok(inst) = ud.borrow::<Instance>() {
                Ok(Some(inst.clone()))
            } else if let Ok(ws) = ud.borrow::<WorkspaceService>() {
                Ok(Some(ws.instance.clone()))
            } else if let Ok(players) = ud.borrow::<PlayersService>() {
                Ok(Some(players.instance.clone()))
            } else {
                Err(mlua::Error::runtime(
                    "Parent must be an Instance, supported Service, or nil",
                ))
            }
        }
        _ => Err(mlua::Error::runtime(
            "Parent must be an Instance, supported Service, or nil",
        )),
    }
}

impl UserData for Instance {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, this| Ok(this.name()));
        fields.add_field_method_set("Name", |lua, this, name: String| {
            this.set_name(&name);
            this.fire_property_changed_if_listening(lua, "Name")?;
            Ok(())
        });

        fields.add_field_method_get("ClassName", |_, this| {
            Ok(this.class_name().as_str().to_string())
        });

        fields.add_field_method_get("Parent", |_, this| Ok(this.parent()));
        fields.add_field_method_set("Parent", |lua, this, parent: Value| {
            let parent = parent_instance_from_lua_value(parent)?;
            this.set_parent_with_lua(parent.as_ref(), lua)?;
            Ok(())
        });

        fields.add_field_method_get("ChildAdded", |_, this| {
            Ok(this.data.lock().unwrap().child_added.clone())
        });
        fields.add_field_method_get("ChildRemoved", |_, this| {
            Ok(this.data.lock().unwrap().child_removed.clone())
        });
        fields.add_field_method_get("AncestryChanged", |_, this| {
            Ok(this.data.lock().unwrap().ancestry_changed.clone())
        });
        fields.add_field_method_get("Destroying", |_, this| {
            Ok(this.data.lock().unwrap().destroying.clone())
        });
        fields.add_field_method_get("AttributeChanged", |_, this| {
            Ok(this.data.lock().unwrap().attribute_changed.clone())
        });

        fields.add_field_method_get("Source", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.script_data.as_ref().map(|s| s.source.clone()))
        });
        fields.add_field_method_set("Source", |lua, this, source: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(script) = &mut data.script_data {
                script.source = source;
                drop(data);
                this.fire_property_changed_if_listening(lua, "Source")?;
            }
            Ok(())
        });

        fields.add_field_method_get("Disabled", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.script_data.as_ref().map(|s| s.disabled))
        });
        fields.add_field_method_set("Disabled", |lua, this, disabled: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(script) = &mut data.script_data {
                script.disabled = disabled;
                drop(data);
                this.fire_property_changed_if_listening(lua, "Disabled")?;
            }
            Ok(())
        });

        // Position: Vector3 for Parts, UDim2 for GUI objects
        fields.add_field_method_get("Position", |lua, this| {
            let data = this.data.lock().unwrap();
            if let Some(part) = &data.part_data {
                Ok(Value::UserData(lua.create_userdata(part.position)?))
            } else if let Some(gui) = &data.gui_data {
                Ok(Value::UserData(lua.create_userdata(gui.position)?))
            } else {
                Ok(Value::Nil)
            }
        });
        fields.add_field_method_set("Position", |_, this, value: Value| {
            let mut data = this.data.lock().unwrap();
            match value {
                Value::UserData(ud) => {
                    if let Ok(pos) = ud.borrow::<Vector3>() {
                        if let Some(part) = &mut data.part_data {
                            part.position = *pos;
                            part.cframe.position = *pos;
                            part.position_dirty = true;
                        }
                    } else if let Ok(pos) = ud.borrow::<UDim2>() {
                        if let Some(gui) = &mut data.gui_data {
                            gui.position = *pos;
                        }
                    }
                }
                _ => {}
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
                part.position_dirty = true;
            }
            Ok(())
        });

        // Size: Vector3 for Parts, UDim2 for GUI objects
        fields.add_field_method_get("Size", |lua, this| {
            let data = this.data.lock().unwrap();
            if let Some(part) = &data.part_data {
                Ok(Value::UserData(lua.create_userdata(part.size)?))
            } else if let Some(gui) = &data.gui_data {
                Ok(Value::UserData(lua.create_userdata(gui.size)?))
            } else {
                Ok(Value::Nil)
            }
        });
        fields.add_field_method_set("Size", |_, this, value: Value| {
            let mut data = this.data.lock().unwrap();
            match value {
                Value::UserData(ud) => {
                    if let Ok(size) = ud.borrow::<Vector3>() {
                        if let Some(part) = &mut data.part_data {
                            part.size = *size;
                            part.size_dirty = true;
                        }
                    } else if let Ok(size) = ud.borrow::<UDim2>() {
                        if let Some(gui) = &mut data.gui_data {
                            gui.size = *size;
                        }
                    }
                }
                _ => {}
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
                part.anchored_dirty = true;
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
                part.can_collide_dirty = true;
            }
            Ok(())
        });

        fields.add_field_method_get("CanQuery", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.can_query))
        });
        fields.add_field_method_set("CanQuery", |_, this, can_query: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.can_query = can_query;
            }
            Ok(())
        });

        fields.add_field_method_get("CollisionGroup", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.collision_group.clone()))
        });
        fields.add_field_method_set("CollisionGroup", |_, this, collision_group: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.collision_group = collision_group;
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

        fields.add_field_method_get("TopSurface", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.top_surface.clone()))
        });
        fields.add_field_method_set("TopSurface", |_, this, surface: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.top_surface = surface;
            }
            Ok(())
        });

        fields.add_field_method_get("BottomSurface", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.bottom_surface.clone()))
        });
        fields.add_field_method_set("BottomSurface", |_, this, surface: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.bottom_surface = surface;
            }
            Ok(())
        });

        fields.add_field_method_get("BrickColor", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.part_data.as_ref().map(|p| p.color))
        });
        fields.add_field_method_set("BrickColor", |_, this, color: Color3| {
            let mut data = this.data.lock().unwrap();
            if let Some(part) = &mut data.part_data {
                part.color = color;
            }
            Ok(())
        });

        fields.add_field_method_get("Velocity", |_, this| {
            let data = this.data.lock().unwrap();
            if let Some(bv) = &data.body_velocity_data {
                Ok(Some(bv.velocity))
            } else {
                Ok(data.part_data.as_ref().map(|p| p.velocity))
            }
        });
        fields.add_field_method_set("Velocity", |_, this, velocity: Vector3| {
            let parent_part = {
                let mut data = this.data.lock().unwrap();
                if let Some(bv) = &mut data.body_velocity_data {
                    bv.velocity = velocity;
                    data.parent.as_ref().and_then(|w| w.upgrade())
                } else {
                    if let Some(part) = &mut data.part_data {
                        part.velocity = velocity;
                        part.velocity_dirty = true;
                    }
                    None
                }
            };
            if let Some(parent_ref) = parent_part {
                let mut parent_data = parent_ref.lock().unwrap();
                if let Some(part) = &mut parent_data.part_data {
                    part.velocity = velocity;
                    part.velocity_dirty = true;
                }
            }
            Ok(())
        });

        fields.add_field_method_get("CustomPhysicalProperties", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .part_data
                .as_ref()
                .and_then(|p| p.custom_physical_properties))
        });
        fields.add_field_method_set(
            "CustomPhysicalProperties",
            |_, this, value: Option<PhysicalProperties>| {
                let mut data = this.data.lock().unwrap();
                if let Some(part) = &mut data.part_data {
                    part.custom_physical_properties = value;
                }
                Ok(())
            },
        );

        fields.add_field_method_get("MaxForce", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.body_velocity_data.as_ref().map(|bv| bv.max_force))
        });
        fields.add_field_method_set("MaxForce", |_, this, max_force: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(bv) = &mut data.body_velocity_data {
                bv.max_force = max_force;
            }
            Ok(())
        });

        fields.add_field_method_get("Force", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.body_force_data.as_ref().map(|bf| bf.force))
        });
        fields.add_field_method_set("Force", |_, this, force: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(bf) = &mut data.body_force_data {
                bf.force = force;
            }
            Ok(())
        });

        fields.add_field_method_get("AngularVelocity", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .body_angular_velocity_data
                .as_ref()
                .map(|bav| bav.angular_velocity))
        });
        fields.add_field_method_set("AngularVelocity", |_, this, angular_velocity: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(bav) = &mut data.body_angular_velocity_data {
                bav.angular_velocity = angular_velocity;
            }
            Ok(())
        });

        fields.add_field_method_get("MaxTorque", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .body_angular_velocity_data
                .as_ref()
                .map(|bav| bav.max_torque))
        });
        fields.add_field_method_set("MaxTorque", |_, this, max_torque: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(bav) = &mut data.body_angular_velocity_data {
                bav.max_torque = max_torque;
            }
            Ok(())
        });

        fields.add_field_method_get("Attachment0", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .align_orientation_data
                .as_ref()
                .and_then(|a| a.attachment0.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });
        fields.add_field_method_set("Attachment0", |_, this, attachment: Option<Instance>| {
            let mut data = this.data.lock().unwrap();
            if let Some(align) = &mut data.align_orientation_data {
                align.attachment0 = attachment.as_ref().map(|a| Arc::downgrade(&a.data));
            }
            Ok(())
        });

        fields.add_field_method_get("Attachment1", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .align_orientation_data
                .as_ref()
                .and_then(|a| a.attachment1.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });
        fields.add_field_method_set("Attachment1", |_, this, attachment: Option<Instance>| {
            let mut data = this.data.lock().unwrap();
            if let Some(align) = &mut data.align_orientation_data {
                align.attachment1 = attachment.as_ref().map(|a| Arc::downgrade(&a.data));
            }
            Ok(())
        });

        fields.add_field_method_get("Responsiveness", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .align_orientation_data
                .as_ref()
                .map(|a| a.responsiveness))
        });
        fields.add_field_method_set("Responsiveness", |_, this, responsiveness: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(align) = &mut data.align_orientation_data {
                align.responsiveness = responsiveness;
            }
            Ok(())
        });

        fields.add_field_method_get("RigidityEnabled", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .align_orientation_data
                .as_ref()
                .map(|a| a.rigidity_enabled))
        });
        fields.add_field_method_set("RigidityEnabled", |_, this, enabled: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(align) = &mut data.align_orientation_data {
                align.rigidity_enabled = enabled;
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
                part.shape_dirty = true;
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

        fields.add_field_method_get("OnServerEvent", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .remote_event_data
                .as_ref()
                .map(|r| r.on_server_event.clone()))
        });

        fields.add_field_method_get("Changed", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .int_value_data
                .as_ref()
                .map(|v| v.changed.clone())
                .or_else(|| data.number_value_data.as_ref().map(|v| v.changed.clone()))
                .or_else(|| data.bool_value_data.as_ref().map(|v| v.changed.clone()))
                .or_else(|| data.string_value_data.as_ref().map(|v| v.changed.clone()))
                .or_else(|| data.object_value_data.as_ref().map(|v| v.changed.clone())))
        });

        fields.add_field_method_get("Value", |lua, this| {
            let data = this.data.lock().unwrap();
            if let Some(v) = &data.int_value_data {
                return Ok(Value::Integer(v.value));
            }
            if let Some(v) = &data.number_value_data {
                return Ok(Value::Number(v.value));
            }
            if let Some(v) = &data.bool_value_data {
                return Ok(Value::Boolean(v.value));
            }
            if let Some(v) = &data.string_value_data {
                return Ok(Value::String(lua.create_string(&v.value)?));
            }
            if let Some(v) = &data.object_value_data {
                if let Some(value_ref) = &v.value {
                    if let Some(strong) = value_ref.upgrade() {
                        let inst = Instance::from_ref(strong);
                        return Ok(Value::UserData(Instance::cached_userdata(lua, &inst)?));
                    }
                }
                return Ok(Value::Nil);
            }
            Ok(Value::Nil)
        });
        fields.add_field_method_set("Value", |lua, this, value: Value| {
            let changed_signal = {
                let mut data = this.data.lock().unwrap();
                if let Some(v) = &mut data.int_value_data {
                    v.value = match &value {
                        Value::Integer(i) => *i,
                        Value::Number(n) => *n as i64,
                        _ => v.value,
                    };
                    Some(v.changed.clone())
                } else if let Some(v) = &mut data.number_value_data {
                    v.value = match &value {
                        Value::Integer(i) => *i as f64,
                        Value::Number(n) => *n,
                        _ => v.value,
                    };
                    Some(v.changed.clone())
                } else if let Some(v) = &mut data.bool_value_data {
                    v.value = match &value {
                        Value::Boolean(b) => *b,
                        _ => v.value,
                    };
                    Some(v.changed.clone())
                } else if let Some(v) = &mut data.string_value_data {
                    if let Value::String(s) = &value {
                        if let Ok(text) = s.to_str() {
                            v.value = text.to_string();
                        }
                    }
                    Some(v.changed.clone())
                } else if let Some(v) = &mut data.object_value_data {
                    match &value {
                        Value::Nil => {
                            v.value = None;
                        }
                        Value::UserData(ud) => {
                            if let Ok(inst) = ud.borrow::<Instance>() {
                                v.value = Some(Arc::downgrade(&inst.data));
                            } else {
                                return Err(mlua::Error::runtime(
                                    "ObjectValue.Value expects Instance or nil",
                                ));
                            }
                        }
                        _ => {
                            return Err(mlua::Error::runtime(
                                "ObjectValue.Value expects Instance or nil",
                            ));
                        }
                    }
                    Some(v.changed.clone())
                } else {
                    None
                }
            };
            if let Some(signal) = changed_signal {
                let threads = signal.fire_as_coroutines(lua, MultiValue::from_iter([value]))?;
                crate::game::lua::events::track_yielded_threads(lua, threads)?;
            }
            Ok(())
        });

        fields.add_field_method_get("Health", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.health))
        });
        fields.add_field_method_set("Health", |lua, this, health: f32| {
            let (old_health, new_health, health_changed, died) = {
                let mut data = this.data.lock().unwrap();
                if let Some(humanoid) = &mut data.humanoid_data {
                    let old = humanoid.health;
                    humanoid.health = health.max(0.0).min(humanoid.max_health);
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
                let threads = health_changed.fire_as_coroutines(
                    lua,
                    mlua::MultiValue::from_iter([Value::Number(new_health as f64)]),
                )?;
                crate::game::lua::events::track_yielded_threads(lua, threads)?;
                if new_health <= 0.0 && old_health > 0.0 {
                    let threads = died.fire_as_coroutines(lua, mlua::MultiValue::new())?;
                    crate::game::lua::events::track_yielded_threads(lua, threads)?;
                }
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

        fields.add_field_method_get("Jump", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.jump_requested))
        });
        fields.add_field_method_set("Jump", |_, this, jump: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                // Roblox semantics: setting Jump true requests one jump.
                if jump {
                    humanoid.jump_requested = true;
                }
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

        fields.add_field_method_get("HealthDisplayDistance", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.health_display_distance))
        });
        fields.add_field_method_set("HealthDisplayDistance", |_, this, value: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.health_display_distance = value.max(0.0);
            }
            Ok(())
        });

        fields.add_field_method_get("NameDisplayDistance", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.name_display_distance))
        });
        fields.add_field_method_set("NameDisplayDistance", |_, this, value: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.name_display_distance = value.max(0.0);
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

        fields.add_field_method_get("MoveDirection", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.move_direction))
        });

        fields.add_field_method_get("Died", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.died.clone()))
        });

        fields.add_field_method_get("HealthChanged", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .humanoid_data
                .as_ref()
                .map(|h| h.health_changed.clone()))
        });

        fields.add_field_method_get("Running", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.running.clone()))
        });

        fields.add_field_method_get("MoveToFinished", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .humanoid_data
                .as_ref()
                .map(|h| h.move_to_finished.clone()))
        });

        fields.add_field_method_get("StateChanged", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.state_changed.clone()))
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
        fields.add_field_method_set("Character", |lua, this, character: Option<Instance>| {
            // Roblox parity: Player.Character is writable from Lua and emits
            // CharacterRemoving(old) followed by CharacterAdded(new) on transitions.
            let (old_character, new_character, character_added, character_removing) = {
                let mut data = this.data.lock().unwrap();
                let Some(player_data) = &mut data.player_data else {
                    return Ok(());
                };

                let old_character = player_data
                    .character
                    .as_ref()
                    .and_then(|w| w.upgrade())
                    .map(Instance::from_ref);
                let new_character = character.clone();

                player_data.character = character.as_ref().map(|c| Arc::downgrade(&c.data));

                (
                    old_character,
                    new_character,
                    player_data.character_added.clone(),
                    player_data.character_removing.clone(),
                )
            };

            let same_character = match (&old_character, &new_character) {
                (Some(old_char), Some(new_char)) => {
                    old_char.data.lock().unwrap().id == new_char.data.lock().unwrap().id
                }
                (None, None) => true,
                _ => false,
            };

            if same_character {
                return Ok(());
            }

            if let Some(old_char) = old_character {
                let threads = character_removing.fire_as_coroutines(
                    lua,
                    mlua::MultiValue::from_iter([Value::UserData(lua.create_userdata(old_char)?)]),
                )?;
                track_yielded_threads(lua, threads)?;
            }

            if let Some(new_char) = new_character {
                let threads = character_added.fire_as_coroutines(
                    lua,
                    mlua::MultiValue::from_iter([Value::UserData(lua.create_userdata(new_char)?)]),
                )?;
                track_yielded_threads(lua, threads)?;
            }

            Ok(())
        });

        fields.add_field_method_get("AnimationId", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.animation_data.as_ref().map(|a| a.animation_id.clone()))
        });
        fields.add_field_method_set("AnimationId", |_, this, animation_id: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(animation) = &mut data.animation_data {
                animation.animation_id = animation_id;
            }
            Ok(())
        });

        fields.add_field_method_get("CharacterAdded", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.player_data.as_ref().map(|p| p.character_added.clone()))
        });

        fields.add_field_method_get("CharacterRemoving", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .player_data
                .as_ref()
                .map(|p| p.character_removing.clone()))
        });

        fields.add_field_method_get("PlayerGui", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .player_data
                .as_ref()
                .and_then(|p| p.player_gui.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
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

        // ========== Weld Properties ==========

        fields.add_field_method_get("Part0", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .weld_data
                .as_ref()
                .and_then(|w| w.part0.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });
        fields.add_field_method_set("Part0", |_, this, part: Option<Instance>| {
            let mut data = this.data.lock().unwrap();
            if let Some(weld) = &mut data.weld_data {
                weld.part0 = part.map(|p| Arc::downgrade(&p.data));
            }
            Ok(())
        });

        fields.add_field_method_get("Part1", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .weld_data
                .as_ref()
                .and_then(|w| w.part1.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });
        fields.add_field_method_set("Part1", |_, this, part: Option<Instance>| {
            let mut data = this.data.lock().unwrap();
            if let Some(weld) = &mut data.weld_data {
                weld.part1 = part.map(|p| Arc::downgrade(&p.data));
            }
            Ok(())
        });

        fields.add_field_method_get("C0", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.weld_data.as_ref().map(|w| w.c0))
        });
        fields.add_field_method_set("C0", |_, this, cframe: CFrame| {
            let mut data = this.data.lock().unwrap();
            if let Some(weld) = &mut data.weld_data {
                weld.c0 = cframe;
            }
            Ok(())
        });

        fields.add_field_method_get("C1", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.weld_data.as_ref().map(|w| w.c1))
        });
        fields.add_field_method_set("C1", |_, this, cframe: CFrame| {
            let mut data = this.data.lock().unwrap();
            if let Some(weld) = &mut data.weld_data {
                weld.c1 = cframe;
            }
            Ok(())
        });

        fields.add_field_method_get("Enabled", |_, this| {
            let data = this.data.lock().unwrap();
            // Return Enabled for Welds, BillboardGui, or existing gui_data.enabled
            if let Some(weld) = &data.weld_data {
                return Ok(Some(weld.enabled));
            }
            if let Some(billboard) = &data.billboard_gui_data {
                return Ok(Some(billboard.enabled));
            }
            if let Some(gui) = &data.gui_data {
                return Ok(Some(gui.enabled));
            }
            Ok(None)
        });

        // ========== BillboardGui Properties ==========

        fields.add_field_method_get("StudsOffset", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.billboard_gui_data.as_ref().map(|b| b.studs_offset))
        });
        fields.add_field_method_set("StudsOffset", |_, this, offset: Vector3| {
            let mut data = this.data.lock().unwrap();
            if let Some(billboard) = &mut data.billboard_gui_data {
                billboard.studs_offset = offset;
            }
            Ok(())
        });

        fields.add_field_method_get("AlwaysOnTop", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.billboard_gui_data.as_ref().map(|b| b.always_on_top))
        });
        fields.add_field_method_set("AlwaysOnTop", |_, this, value: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(billboard) = &mut data.billboard_gui_data {
                billboard.always_on_top = value;
            }
            Ok(())
        });

        fields.add_field_method_get("Adornee", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .billboard_gui_data
                .as_ref()
                .and_then(|b| b.adornee.as_ref())
                .and_then(|w| w.upgrade())
                .map(Instance::from_ref))
        });
        fields.add_field_method_set("Adornee", |_, this, part: Option<Instance>| {
            let mut data = this.data.lock().unwrap();
            if let Some(billboard) = &mut data.billboard_gui_data {
                billboard.adornee = part.map(|p| Arc::downgrade(&p.data));
            }
            Ok(())
        });

        // ========== GUI Properties ==========

        // AnchorPoint (GuiObject)
        fields.add_field_method_get("AnchorPoint", |lua, this| {
            let data = this.data.lock().unwrap();
            if let Some(gui) = &data.gui_data {
                let table = lua.create_table()?;
                table.set("X", gui.anchor_point.0)?;
                table.set("Y", gui.anchor_point.1)?;
                Ok(Value::Table(table))
            } else {
                Ok(Value::Nil)
            }
        });
        fields.add_field_method_set("AnchorPoint", |_, this, value: Value| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                if let Value::Table(t) = value {
                    let x: f32 = t.get("X").unwrap_or(0.0);
                    let y: f32 = t.get("Y").unwrap_or(0.0);
                    gui.anchor_point = (x, y);
                }
            }
            Ok(())
        });

        // Rotation (GuiObject) - in degrees
        fields.add_field_method_get("Rotation", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.rotation))
        });
        fields.add_field_method_set("Rotation", |_, this, rotation: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.rotation = rotation;
            }
            Ok(())
        });

        // ZIndex (GuiObject)
        fields.add_field_method_get("ZIndex", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.z_index))
        });
        fields.add_field_method_set("ZIndex", |_, this, z_index: i32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.z_index = z_index;
            }
            Ok(())
        });

        // LayoutOrder (GuiObject)
        fields.add_field_method_get("LayoutOrder", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.layout_order))
        });
        fields.add_field_method_set("LayoutOrder", |_, this, layout_order: i32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.layout_order = layout_order;
            }
            Ok(())
        });

        // Visible (GuiObject)
        fields.add_field_method_get("Visible", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.visible))
        });
        fields.add_field_method_set("Visible", |_, this, visible: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.visible = visible;
            }
            Ok(())
        });

        // BackgroundColor3 (GuiObject)
        fields.add_field_method_get("BackgroundColor3", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.background_color))
        });
        fields.add_field_method_set("BackgroundColor3", |_, this, color: Color3| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.background_color = color;
            }
            Ok(())
        });

        // BackgroundTransparency (GuiObject)
        fields.add_field_method_get("BackgroundTransparency", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.background_transparency))
        });
        fields.add_field_method_set("BackgroundTransparency", |_, this, transparency: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.background_transparency = transparency.clamp(0.0, 1.0);
            }
            Ok(())
        });

        // BorderColor3 (GuiObject)
        fields.add_field_method_get("BorderColor3", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.border_color))
        });
        fields.add_field_method_set("BorderColor3", |_, this, color: Color3| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.border_color = color;
            }
            Ok(())
        });

        // BorderSizePixel (GuiObject)
        fields.add_field_method_get("BorderSizePixel", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.border_size_pixel))
        });
        fields.add_field_method_set("BorderSizePixel", |_, this, size: i32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.border_size_pixel = size.max(0);
            }
            Ok(())
        });

        // Text (TextLabel, TextButton)
        fields.add_field_method_get("Text", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.text.clone()))
        });
        fields.add_field_method_set("Text", |_, this, text: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.text = Some(text);
            }
            Ok(())
        });

        // TextColor3 (TextLabel, TextButton)
        fields.add_field_method_get("TextColor3", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.text_color))
        });
        fields.add_field_method_set("TextColor3", |_, this, color: Color3| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.text_color = Some(color);
            }
            Ok(())
        });

        // TextSize (TextLabel, TextButton)
        fields.add_field_method_get("TextSize", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.text_size))
        });
        fields.add_field_method_set("TextSize", |_, this, size: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.text_size = Some(size.max(1.0));
            }
            Ok(())
        });

        // TextTransparency (TextLabel, TextButton)
        fields.add_field_method_get("TextTransparency", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.text_transparency))
        });
        fields.add_field_method_set("TextTransparency", |_, this, transparency: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.text_transparency = Some(transparency.clamp(0.0, 1.0));
            }
            Ok(())
        });

        // TextStrokeTransparency (TextLabel, TextButton)
        fields.add_field_method_get("TextStrokeTransparency", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .gui_data
                .as_ref()
                .and_then(|g| g.text_stroke_transparency))
        });
        fields.add_field_method_set("TextStrokeTransparency", |_, this, transparency: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.text_stroke_transparency = Some(transparency.clamp(0.0, 1.0));
            }
            Ok(())
        });

        // Font (TextLabel, TextButton)
        fields.add_field_method_get("Font", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.font.clone()))
        });
        fields.add_field_method_set("Font", |_, this, font: Value| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                if let Value::String(s) = font {
                    if let Ok(font_name) = s.to_str() {
                        gui.font = Some(font_name.to_string());
                    }
                }
            }
            Ok(())
        });

        // TextScaled (TextLabel, TextButton)
        fields.add_field_method_get("TextScaled", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.text_scaled))
        });
        fields.add_field_method_set("TextScaled", |_, this, scaled: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.text_scaled = scaled;
            }
            Ok(())
        });

        // TextXAlignment (TextLabel, TextButton)
        fields.add_field_method_get("TextXAlignment", |lua, this| {
            let data = this.data.lock().unwrap();
            if let Some(gui) = &data.gui_data {
                let alignment = match gui.text_x_alignment {
                    TextXAlignment::Left => "Left",
                    TextXAlignment::Center => "Center",
                    TextXAlignment::Right => "Right",
                };
                Ok(Value::String(lua.create_string(alignment)?))
            } else {
                Ok(Value::Nil)
            }
        });
        fields.add_field_method_set("TextXAlignment", |_, this, alignment: Value| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                if let Value::String(s) = alignment {
                    if let Ok(str_val) = s.to_str() {
                        gui.text_x_alignment = match str_val.as_ref() {
                            "Left" => TextXAlignment::Left,
                            "Right" => TextXAlignment::Right,
                            _ => TextXAlignment::Center,
                        };
                    }
                }
            }
            Ok(())
        });

        // TextYAlignment (TextLabel, TextButton)
        fields.add_field_method_get("TextYAlignment", |lua, this| {
            let data = this.data.lock().unwrap();
            if let Some(gui) = &data.gui_data {
                let alignment = match gui.text_y_alignment {
                    TextYAlignment::Top => "Top",
                    TextYAlignment::Center => "Center",
                    TextYAlignment::Bottom => "Bottom",
                };
                Ok(Value::String(lua.create_string(alignment)?))
            } else {
                Ok(Value::Nil)
            }
        });
        fields.add_field_method_set("TextYAlignment", |_, this, alignment: Value| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                if let Value::String(s) = alignment {
                    if let Ok(str_val) = s.to_str() {
                        gui.text_y_alignment = match str_val.as_ref() {
                            "Top" => TextYAlignment::Top,
                            "Bottom" => TextYAlignment::Bottom,
                            _ => TextYAlignment::Center,
                        };
                    }
                }
            }
            Ok(())
        });

        // Image (ImageLabel, ImageButton)
        fields.add_field_method_get("Image", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.image.clone()))
        });
        fields.add_field_method_set("Image", |_, this, image: String| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.image = Some(image);
            }
            Ok(())
        });

        // ImageColor3 (ImageLabel, ImageButton)
        fields.add_field_method_get("ImageColor3", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.image_color))
        });
        fields.add_field_method_set("ImageColor3", |_, this, color: Color3| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.image_color = Some(color);
            }
            Ok(())
        });

        // ImageTransparency (ImageLabel, ImageButton)
        fields.add_field_method_get("ImageTransparency", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.image_transparency))
        });
        fields.add_field_method_set("ImageTransparency", |_, this, transparency: f32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.image_transparency = Some(transparency.clamp(0.0, 1.0));
            }
            Ok(())
        });

        // DisplayOrder (ScreenGui)
        fields.add_field_method_get("DisplayOrder", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.display_order))
        });
        fields.add_field_method_set("DisplayOrder", |_, this, order: i32| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.display_order = order;
            }
            Ok(())
        });

        // IgnoreGuiInset (ScreenGui)
        fields.add_field_method_get("IgnoreGuiInset", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.ignore_gui_inset))
        });
        fields.add_field_method_set("IgnoreGuiInset", |_, this, ignore: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.ignore_gui_inset = ignore;
            }
            Ok(())
        });

        // Enabled (ScreenGui)
        fields.add_field_method_get("Enabled", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.enabled))
        });
        fields.add_field_method_set("Enabled", |_, this, enabled: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.enabled = enabled;
            }
            Ok(())
        });

        // ResetOnSpawn (ScreenGui)
        fields.add_field_method_get("ResetOnSpawn", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().map(|g| g.reset_on_spawn))
        });
        fields.add_field_method_set("ResetOnSpawn", |_, this, reset_on_spawn: bool| {
            let mut data = this.data.lock().unwrap();
            if let Some(gui) = &mut data.gui_data {
                gui.reset_on_spawn = reset_on_spawn;
            }
            Ok(())
        });

        // CornerRadius (UICorner)
        fields.add_field_method_get("CornerRadius", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.ui_corner_data.as_ref().map(|c| c.corner_radius))
        });
        fields.add_field_method_set("CornerRadius", |_, this, corner_radius: UDim| {
            let mut data = this.data.lock().unwrap();
            if let Some(ui_corner) = &mut data.ui_corner_data {
                ui_corner.corner_radius = corner_radius;
            }
            Ok(())
        });

        // MouseButton1Click (GuiButton)
        fields.add_field_method_get("MouseButton1Click", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .gui_data
                .as_ref()
                .and_then(|g| g.mouse_button1_click.clone()))
        });

        // MouseButton1Down (GuiButton)
        fields.add_field_method_get("MouseButton1Down", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .gui_data
                .as_ref()
                .and_then(|g| g.mouse_button1_down.clone()))
        });

        // MouseButton1Up (GuiButton)
        fields.add_field_method_get("MouseButton1Up", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data
                .gui_data
                .as_ref()
                .and_then(|g| g.mouse_button1_up.clone()))
        });

        // MouseEnter (GuiButton)
        fields.add_field_method_get("MouseEnter", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.mouse_enter.clone()))
        });

        // MouseLeave (GuiButton)
        fields.add_field_method_get("MouseLeave", |_, this| {
            let data = this.data.lock().unwrap();
            Ok(data.gui_data.as_ref().and_then(|g| g.mouse_leave.clone()))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Clone", |_, this, ()| Ok(this.clone_instance()));

        methods.add_method("Destroy", |lua, this, ()| this.destroy(lua));

        methods.add_method("ClearAllChildren", |lua, this, ()| {
            // Roblox parity: ClearAllChildren destroys all direct children.
            // `destroy` recursively destroys descendants and updates parent/child signals.
            for child in this.get_children() {
                child.destroy(lua)?;
            }
            Ok(())
        });

        methods.add_method("FireAllClients", |lua, this, args: MultiValue| {
            if this.class_name() != ClassName::RemoteEvent {
                return Ok(());
            }

            let args_vec: Vec<Value> = args.into_iter().collect();
            let payload =
                crate::game::lua::services::remote_event::lua_args_to_payload(lua, &args_vec)?;
            let game_ud: mlua::AnyUserData = lua.globals().get("__clawblox_game")?;
            let game = game_ud.borrow::<Game>()?;
            game.remote_event_service()
                .fire_all_clients(this.name(), payload, true);
            Ok(())
        });

        methods.add_method("FireClient", |lua, this, args: MultiValue| {
            if this.class_name() != ClassName::RemoteEvent {
                return Ok(());
            }

            let mut iter = args.into_iter();
            let Some(player_value) = iter.next() else {
                return Ok(());
            };
            let player: Instance = mlua::FromLua::from_lua(player_value, lua)?;
            let target_user_id = {
                let data = player.data.lock().unwrap();
                data.player_data.as_ref().map(|p| p.user_id)
            };
            let Some(target_user_id) = target_user_id else {
                return Ok(());
            };

            let rest: Vec<Value> = iter.collect();
            let payload =
                crate::game::lua::services::remote_event::lua_args_to_payload(lua, &rest)?;
            let game_ud: mlua::AnyUserData = lua.globals().get("__clawblox_game")?;
            let game = game_ud.borrow::<Game>()?;
            game.remote_event_service()
                .fire_client(this.name(), payload, true, target_user_id);
            Ok(())
        });

        methods.add_method(
            "FindFirstChild",
            |_, this, (name, recursive): (String, Option<bool>)| {
                Ok(this.find_first_child(&name, recursive.unwrap_or(false)))
            },
        );

        methods.add_method("FindFirstChildOfClass", |_, this, class_name: String| {
            Ok(this.find_first_child_of_class(&class_name))
        });

        methods.add_method(
            "FindFirstChildWhichIsA",
            |_, this, (class_name, recursive): (String, Option<bool>)| {
                if recursive.unwrap_or(false) {
                    Ok(this
                        .get_descendants()
                        .into_iter()
                        .find(|child| child.is_a(&class_name)))
                } else {
                    Ok(this
                        .get_children()
                        .into_iter()
                        .find(|child| child.is_a(&class_name)))
                }
            },
        );

        methods.add_method("GetChildren", |_, this, ()| Ok(this.get_children()));

        methods.add_method("GetDescendants", |_, this, ()| Ok(this.get_descendants()));

        methods.add_method("GetDebugId", |_, this, _scope_length: Option<u32>| {
            Ok(format!("RBX{:016x}", this.id().0))
        });

        methods.add_method("IsA", |_, this, class_name: String| {
            Ok(this.is_a(&class_name))
        });

        methods.add_method("IsDescendantOf", |_, this, ancestor: Value| {
            let ancestor = match ancestor {
                Value::UserData(ud) => match ud.borrow::<Instance>() {
                    Ok(inst) => inst.clone(),
                    Err(_) => return Ok(false),
                },
                _ => return Ok(false),
            };
            Ok(this.is_descendant_of(&ancestor))
        });

        methods.add_method(
            "SetAttribute",
            |lua, this, (name, value): (String, Value)| {
                fn lua_value_to_attr(value: &Value) -> mlua::Result<AttributeValue> {
                    match value {
                        Value::Nil => Ok(AttributeValue::Nil),
                        Value::Boolean(b) => Ok(AttributeValue::Bool(*b)),
                        Value::Integer(n) => Ok(AttributeValue::Number(*n as f64)),
                        Value::Number(n) => Ok(AttributeValue::Number(*n)),
                        Value::String(s) => Ok(AttributeValue::String(s.to_str()?.to_string())),
                        Value::UserData(ud) => {
                            if let Ok(v) = ud.borrow::<Vector3>() {
                                Ok(AttributeValue::Vector3(*v))
                            } else if let Ok(c) = ud.borrow::<Color3>() {
                                Ok(AttributeValue::Color3(*c))
                            } else {
                                Err(mlua::Error::runtime("Unsupported attribute type"))
                            }
                        }
                        Value::Table(t) => {
                            let mut items = Vec::new();
                            for pair in t.clone().sequence_values::<Value>() {
                                items.push(lua_value_to_attr(&pair?)?);
                            }
                            Ok(AttributeValue::Table(items))
                        }
                        _ => Err(mlua::Error::runtime("Unsupported attribute type")),
                    }
                }
                let attr_value = lua_value_to_attr(&value)?;
                this.set_attribute_with_lua(lua, &name, attr_value)?;
                Ok(())
            },
        );

        methods.add_method("GetAttributeChangedSignal", |_, this, name: String| {
            Ok(this.ensure_attribute_signal(&name))
        });

        methods.add_method("GetPropertyChangedSignal", |_, this, name: String| {
            Ok(this.ensure_property_signal(&name))
        });

        methods.add_async_method(
            "WaitForChild",
            |lua, this, (name, timeout): (String, Option<f64>)| {
                let this = this.clone();
                async move {
                    let start = std::time::Instant::now();
                    let timeout = timeout.map(|t| t.max(0.0));
                    let name_for_poll = name.clone();

                    let child = futures_util::future::poll_fn(move |_cx| {
                        if let Some(child) = this.find_first_child(&name_for_poll, false) {
                            return std::task::Poll::Ready(Some(child));
                        }

                        if let Some(t) = timeout {
                            if start.elapsed().as_secs_f64() >= t {
                                return std::task::Poll::Ready(None);
                            }
                        }

                        std::task::Poll::Pending
                    })
                    .await;

                    match child {
                        Some(child) => Ok(Value::UserData(lua.create_userdata(child)?)),
                        None => Ok(Value::Nil),
                    }
                }
            },
        );

        methods.add_method("GetAttribute", |lua, this, name: String| {
            fn attr_to_lua(lua: &mlua::Lua, attr: AttributeValue) -> mlua::Result<Value> {
                match attr {
                    AttributeValue::Nil => Ok(Value::Nil),
                    AttributeValue::Bool(b) => Ok(Value::Boolean(b)),
                    AttributeValue::Number(n) => Ok(Value::Number(n)),
                    AttributeValue::String(s) => Ok(Value::String(lua.create_string(&s)?)),
                    AttributeValue::Vector3(v) => Ok(Value::UserData(lua.create_userdata(v)?)),
                    AttributeValue::Color3(c) => Ok(Value::UserData(lua.create_userdata(c)?)),
                    AttributeValue::Table(items) => {
                        let t = lua.create_table()?;
                        for (i, item) in items.into_iter().enumerate() {
                            t.set(i + 1, attr_to_lua(lua, item)?)?;
                        }
                        Ok(Value::Table(t))
                    }
                }
            }
            match this.get_attribute(&name) {
                Some(attr) => attr_to_lua(lua, attr),
                None => Ok(Value::Nil),
            }
        });

        methods.add_method("GetAttributes", |lua, this, ()| {
            fn attr_to_lua_val(lua: &mlua::Lua, attr: AttributeValue) -> mlua::Result<Value> {
                match attr {
                    AttributeValue::Nil => Ok(Value::Nil),
                    AttributeValue::Bool(b) => Ok(Value::Boolean(b)),
                    AttributeValue::Number(n) => Ok(Value::Number(n)),
                    AttributeValue::String(s) => Ok(Value::String(lua.create_string(&s)?)),
                    AttributeValue::Vector3(v) => Ok(Value::UserData(lua.create_userdata(v)?)),
                    AttributeValue::Color3(c) => Ok(Value::UserData(lua.create_userdata(c)?)),
                    AttributeValue::Table(items) => {
                        let t = lua.create_table()?;
                        for (i, item) in items.into_iter().enumerate() {
                            t.set(i + 1, attr_to_lua_val(lua, item)?)?;
                        }
                        Ok(Value::Table(t))
                    }
                }
            }
            let table = lua.create_table()?;
            for (key, value) in this.get_attributes() {
                table.set(key, attr_to_lua_val(lua, value)?)?;
            }
            Ok(table)
        });

        // Tag methods (Roblox CollectionService-style tags)
        methods.add_method("AddTag", |_, this, tag: String| {
            this.add_tag(&tag);
            Ok(())
        });

        methods.add_method("HasTag", |_, this, tag: String| Ok(this.has_tag(&tag)));

        methods.add_method("RemoveTag", |_, this, tag: String| {
            this.remove_tag(&tag);
            Ok(())
        });

        methods.add_method("GetTags", |lua, this, ()| {
            let tags = this.get_tags();
            let table = lua.create_table()?;
            for (i, tag) in tags.iter().enumerate() {
                table.set(i + 1, tag.clone())?;
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
                let threads = health_changed.fire_as_coroutines(
                    lua,
                    mlua::MultiValue::from_iter([Value::Number(new_health as f64)]),
                )?;
                crate::game::lua::events::track_yielded_threads(lua, threads)?;
                if new_health <= 0.0 && old_health > 0.0 {
                    let threads = died.fire_as_coroutines(lua, mlua::MultiValue::new())?;
                    crate::game::lua::events::track_yielded_threads(lua, threads)?;
                }
            }
            Ok(())
        });

        methods.add_method("Move", |_, this, (direction, _relative): (Vector3, Option<bool>)| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.move_input_direction = direction;
            }
            Ok(())
        });

        methods.add_method(
            "MoveTo",
            |_, this, (position, _part): (Vector3, Option<Instance>)| {
                let mut data = this.data.lock().unwrap();
                let id = data.id.0;
                if let Some(humanoid) = &mut data.humanoid_data {
                    let _ = id;
                    humanoid.move_to_target = Some(position);
                } else {
                    // No-op: MoveTo on non-humanoid instance.
                }
                Ok(())
            },
        );

        methods.add_method("CancelMoveTo", |_, this, ()| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.move_to_target = None;
                humanoid.cancel_move_to = true; // Signal to clear physics target
            }
            Ok(())
        });

        methods.add_method("Jump", |_, this, ()| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.jump_requested = true;
            }
            Ok(())
        });

        methods.add_method("GetState", |_, this, ()| {
            let data = this.data.lock().unwrap();
            Ok(data.humanoid_data.as_ref().map(|h| h.state))
        });

        methods.add_method("ChangeState", |_, this, state: HumanoidStateType| {
            let mut data = this.data.lock().unwrap();
            if let Some(humanoid) = &mut data.humanoid_data {
                humanoid.state = state;
            }
            Ok(())
        });

        methods.add_method("LoadAnimation", |lua, this, animation: Instance| {
            let class_name = this.class_name();
            if class_name == ClassName::Animator {
                return this.load_animation_track_for_animator(lua, &animation);
            }
            if class_name == ClassName::Humanoid {
                let animator = this.ensure_animator_child_for_humanoid();
                return animator.load_animation_track_for_animator(lua, &animation);
            }
            Err(mlua::Error::runtime(
                "LoadAnimation is supported on Humanoid and Animator",
            ))
        });

        methods.add_method("GetPlayingAnimationTracks", |lua, this, ()| {
            if this.class_name() != ClassName::Animator {
                return Err(mlua::Error::runtime(
                    "GetPlayingAnimationTracks is supported on Animator",
                ));
            }
            let animator_id = this.id().0;
            let tracks = lua
                .app_data_ref::<AnimationScheduler>()
                .map(|scheduler| scheduler.playing_tracks_for_animator(animator_id))
                .unwrap_or_default();
            lua.create_sequence_from(tracks)
        });

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

        methods.add_method("GetMass", |_, this, ()| {
            let data = this.data.lock().unwrap();
            let Some(part) = &data.part_data else {
                return Err(mlua::Error::runtime(
                    "GetMass is only supported on BasePart",
                ));
            };

            let density = match part.material {
                Material::Wood => 0.35,
                Material::Metal => 0.7,
                Material::Glass => 0.6,
                Material::Neon => 0.7,
                Material::Concrete => 0.7,
                Material::Brick => 0.7,
                Material::Granite => 0.7,
                Material::Grass => 0.7,
                Material::Ice => 0.65,
                Material::Sand => 0.65,
                Material::Fabric => 0.3,
                Material::Marble => 0.7,
                Material::Slate => 0.7,
                Material::SmoothPlastic => 0.7,
                Material::ForceField => 0.0,
                Material::Plastic => 0.7,
            };

            let sx = part.size.x.max(0.0);
            let sy = part.size.y.max(0.0);
            let sz = part.size.z.max(0.0);
            let volume = match part.shape {
                PartType::Block => sx * sy * sz,
                PartType::Wedge => 0.5 * sx * sy * sz,
                PartType::Ball => {
                    let r = sx * 0.5;
                    (4.0 / 3.0) * std::f32::consts::PI * r * r * r
                }
                PartType::Cylinder => {
                    let r = sx * 0.5;
                    std::f32::consts::PI * r * r * sy
                }
            };
            Ok(volume * density)
        });

        methods.add_method("SetNetworkOwner", |_, this, _owner: Value| {
            if this.data.lock().unwrap().part_data.is_none() {
                return Err(mlua::Error::runtime(
                    "SetNetworkOwner is only supported on BasePart",
                ));
            }
            // Roblox parity: server scripts often force server ownership with nil.
            // Clawblox runs authoritative server simulation, so ownership is a no-op.
            Ok(())
        });

        methods.add_method("SetNetworkOwnershipAuto", |_, this, _enabled: Option<bool>| {
            if this.data.lock().unwrap().part_data.is_none() {
                return Err(mlua::Error::runtime(
                    "SetNetworkOwnershipAuto is only supported on BasePart",
                ));
            }
            // Same parity rationale as SetNetworkOwner: network ownership is not modeled yet.
            Ok(())
        });

        methods.add_method("LoadCharacter", |lua, this, ()| {
            let (player_name, old_character, character_added, character_removing) = {
                let data = this.data.lock().unwrap();
                let Some(pdata) = data.player_data.as_ref() else {
                    return Ok(());
                };
                (
                    data.name.clone(),
                    pdata
                        .character
                        .as_ref()
                        .and_then(|w| w.upgrade())
                        .map(Instance::from_ref),
                    pdata.character_added.clone(),
                    pdata.character_removing.clone(),
                )
            };

            if let Some(old_char) = old_character {
                let threads = character_removing.fire_as_coroutines(
                    lua,
                    mlua::MultiValue::from_iter([Value::UserData(
                        lua.create_userdata(old_char.clone())?,
                    )]),
                )?;
                crate::game::lua::events::track_yielded_threads(lua, threads)?;
                old_char.set_parent(None);
            }

            let character = Instance::from_data(InstanceData::new_model(&player_name));

            let mut hrp_data = InstanceData::new_part("HumanoidRootPart");
            if let Some(part) = &mut hrp_data.part_data {
                part.size = Vector3::new(2.0, 5.0, 2.0);
                part.position = Vector3::new(0.0, 6.0, 0.0);
                part.anchored = false;
                part.shape = PartType::Cylinder;
                part.color = Color3::new(0.9, 0.45, 0.3);
            }
            hrp_data.attributes.insert(
                "ModelUrl".to_string(),
                AttributeValue::String("asset://player.glb".to_string()),
            );
            let hrp = Instance::from_data(hrp_data);
            hrp.set_parent(Some(&character));

            let humanoid = Instance::from_data(InstanceData::new_humanoid("Humanoid"));
            humanoid.set_parent(Some(&character));

            {
                let mut char_data = character.data.lock().unwrap();
                if let Some(model) = &mut char_data.model_data {
                    model.primary_part = Some(Arc::downgrade(&hrp.data));
                }
            }

            {
                let mut player_data = this.data.lock().unwrap();
                if let Some(pdata) = &mut player_data.player_data {
                    pdata.character = Some(Arc::downgrade(&character.data));
                }
            }

            let game_ud: mlua::AnyUserData = lua.globals().get("__clawblox_game")?;
            let game_ref = game_ud.borrow::<Game>()?;
            game_ref.workspace().add_child(character.clone());

            let threads = character_added.fire_as_coroutines(
                lua,
                mlua::MultiValue::from_iter([Value::UserData(
                    lua.create_userdata(character.clone())?,
                )]),
            )?;
            crate::game::lua::events::track_yielded_threads(lua, threads)?;

            Ok(())
        });

        methods.add_method("Kick", |lua, this, message: Option<String>| {
            // Get user_id from this player instance
            let user_id = {
                let data = this.data.lock().unwrap();
                data.player_data.as_ref().map(|p| p.user_id)
            };

            if let Some(user_id) = user_id {
                // Get game reference from Lua globals and queue kick request
                let game_ud: mlua::Result<mlua::AnyUserData> = lua.globals().get("__clawblox_game");
                if let Ok(ud) = game_ud {
                    if let Ok(game) = ud.borrow::<Game>() {
                        game.queue_kick(user_id, message.clone());
                        if let Some(msg) = &message {
                            eprintln!("[Player] Kick requested for user_id={}: {}", user_id, msg);
                        } else {
                            eprintln!("[Player] Kick requested for user_id={}", user_id);
                        }
                    } else {
                        eprintln!("[Player] Kick failed: could not borrow Game from UserData");
                    }
                } else {
                    eprintln!("[Player] Kick failed: game reference not found in Lua globals");
                }
            }
            Ok(())
        });

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
            |_, (class_name, parent): (String, Value)| -> Result<Instance> {
                let instance = match class_name.as_str() {
                    "Part" => Instance::from_data(InstanceData::new_part("Part")),
                    "Model" => Instance::from_data(InstanceData::new_model("Model")),
                    "Humanoid" => Instance::from_data(InstanceData::new_humanoid("Humanoid")),
                    "Animation" => Instance::from_data(InstanceData::new_animation("Animation")),
                    "Animator" => Instance::from_data(InstanceData::new_animator("Animator")),
                    "Folder" => Instance::new(ClassName::Folder, "Folder"),
                    "Script" => Instance::from_data(InstanceData::new_script("Script")),
                    "ModuleScript" => {
                        Instance::from_data(InstanceData::new_module_script("ModuleScript"))
                    }
                    "ServerScriptService" => {
                        Instance::from_data(InstanceData::new_server_script_service())
                    }
                    "RemoteEvent" => {
                        Instance::from_data(InstanceData::new_remote_event("RemoteEvent"))
                    }
                    "IntValue" => Instance::from_data(InstanceData::new_int_value("IntValue")),
                    "NumberValue" => {
                        Instance::from_data(InstanceData::new_number_value("NumberValue"))
                    }
                    "BoolValue" => Instance::from_data(InstanceData::new_bool_value("BoolValue")),
                    "StringValue" => {
                        Instance::from_data(InstanceData::new_string_value("StringValue"))
                    }
                    "ObjectValue" => {
                        Instance::from_data(InstanceData::new_object_value("ObjectValue"))
                    }
                    "BodyVelocity" => {
                        Instance::from_data(InstanceData::new_body_velocity("BodyVelocity"))
                    }
                    "BodyForce" => {
                        Instance::from_data(InstanceData::new_body_force("BodyForce"))
                    }
                    "BodyAngularVelocity" => Instance::from_data(
                        InstanceData::new_body_angular_velocity("BodyAngularVelocity"),
                    ),
                    "Attachment" => {
                        Instance::from_data(InstanceData::new_attachment("Attachment"))
                    }
                    "AlignOrientation" => Instance::from_data(
                        InstanceData::new_align_orientation("AlignOrientation"),
                    ),
                    // Constraints
                    "Weld" => Instance::from_data(InstanceData::new_weld("Weld")),
                    // 3D GUI
                    "BillboardGui" => {
                        Instance::from_data(InstanceData::new_billboard_gui("BillboardGui"))
                    }
                    // GUI classes
                    "ScreenGui" => Instance::from_data(InstanceData::new_screen_gui("ScreenGui")),
                    "Frame" => Instance::from_data(InstanceData::new_frame("Frame")),
                    "TextLabel" => Instance::from_data(InstanceData::new_text_label("TextLabel")),
                    "TextButton" => {
                        Instance::from_data(InstanceData::new_text_button("TextButton"))
                    }
                    "ImageLabel" => {
                        Instance::from_data(InstanceData::new_image_label("ImageLabel"))
                    }
                    "ImageButton" => {
                        Instance::from_data(InstanceData::new_image_button("ImageButton"))
                    }
                    "UICorner" => Instance::from_data(InstanceData::new_uicorner("UICorner")),
                    _ => {
                        return Err(mlua::Error::runtime(format!(
                            "Unknown class '{}'",
                            class_name
                        )))
                    }
                };

                let parent = parent_instance_from_lua_value(parent)?;
                instance.set_parent(parent.as_ref());

                Ok(instance)
            },
        )?,
    )?;

    lua.globals().set("Instance", instance_table)?;

    Ok(())
}
