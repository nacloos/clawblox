use mlua::{FromLua, Lua, Result, UserData, UserDataFields, UserDataMethods, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::game::constants::humanoid as humanoid_consts;
use super::events::{create_signal, RBXScriptSignal};
use super::runtime::Game;
use super::services::WorkspaceService;
use super::types::{CFrame, Color3, Material, PartType, UDim2, Vector3};

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
    // Constraints
    Weld,
    // GUI classes
    BillboardGui,
    PlayerGui,
    ScreenGui,
    Frame,
    TextLabel,
    TextButton,
    ImageLabel,
    ImageButton,
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
            ClassName::Weld => "Weld",
            ClassName::BillboardGui => "BillboardGui",
            ClassName::PlayerGui => "PlayerGui",
            ClassName::ScreenGui => "ScreenGui",
            ClassName::Frame => "Frame",
            ClassName::TextLabel => "TextLabel",
            ClassName::TextButton => "TextButton",
            ClassName::ImageLabel => "ImageLabel",
            ClassName::ImageButton => "ImageButton",
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
            // Constraints
            "Constraint" => matches!(self, ClassName::Weld),
            "Weld" => matches!(self, ClassName::Weld),
            // BillboardGui (3D GUI attached to parts)
            "BillboardGui" => matches!(self, ClassName::BillboardGui),
            // GUI hierarchy: GuiBase2d is base for all 2D GUI elements
            "GuiBase2d" => matches!(
                self,
                ClassName::ScreenGui
                    | ClassName::Frame
                    | ClassName::TextLabel
                    | ClassName::TextButton
                    | ClassName::ImageLabel
                    | ClassName::ImageButton
            ),
            // LayerCollector: ScreenGui inherits from this
            "LayerCollector" => matches!(self, ClassName::ScreenGui),
            // GuiObject: Frame, TextLabel, TextButton, ImageLabel, ImageButton
            "GuiObject" => matches!(
                self,
                ClassName::Frame
                    | ClassName::TextLabel
                    | ClassName::TextButton
                    | ClassName::ImageLabel
                    | ClassName::ImageButton
            ),
            // GuiButton: TextButton, ImageButton
            "GuiButton" => matches!(self, ClassName::TextButton | ClassName::ImageButton),
            "PlayerGui" => matches!(self, ClassName::PlayerGui),
            "ScreenGui" => matches!(self, ClassName::ScreenGui),
            "Frame" => matches!(self, ClassName::Frame),
            "TextLabel" => matches!(self, ClassName::TextLabel),
            "TextButton" => matches!(self, ClassName::TextButton),
            "ImageLabel" => matches!(self, ClassName::ImageLabel),
            "ImageButton" => matches!(self, ClassName::ImageButton),
            _ => self.as_str() == class_name,
        }
    }

    /// Check if this class can be a valid parent for GUI elements
    pub fn can_contain_gui(&self) -> bool {
        matches!(
            self,
            ClassName::PlayerGui
                | ClassName::ScreenGui
                | ClassName::Frame
                | ClassName::TextLabel
                | ClassName::TextButton
                | ClassName::ImageLabel
                | ClassName::ImageButton
        )
    }

    /// Check if this is a GUI class that requires a GUI parent
    pub fn is_gui_object(&self) -> bool {
        matches!(
            self,
            ClassName::ScreenGui
                | ClassName::Frame
                | ClassName::TextLabel
                | ClassName::TextButton
                | ClassName::ImageLabel
                | ClassName::ImageButton
        )
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
    pub destroying: RBXScriptSignal,
    pub attribute_changed: RBXScriptSignal,

    pub part_data: Option<PartData>,
    pub humanoid_data: Option<HumanoidData>,
    pub player_data: Option<PlayerData>,
    pub model_data: Option<ModelData>,
    pub gui_data: Option<GuiObjectData>,
    pub weld_data: Option<WeldData>,
    pub billboard_gui_data: Option<BillboardGuiData>,

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
    pub position_dirty: bool,

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
            position_dirty: false,
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
    /// Flag to cancel movement (set by CancelMoveTo)
    pub cancel_move_to: bool,

    pub died: RBXScriptSignal,
    pub health_changed: RBXScriptSignal,
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
            hip_height: humanoid_consts::DEFAULT_HIP_HEIGHT,
            move_to_target: None,
            cancel_move_to: false,
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

impl Default for ModelData {
    fn default() -> Self {
        Self { primary_part: None }
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
            text_scaled: false,
            text_x_alignment: TextXAlignment::default(),
            text_y_alignment: TextYAlignment::default(),
            image: None,
            image_color: None,
            image_transparency: None,
            display_order: 0,
            ignore_gui_inset: false,
            enabled: true,
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
            destroying: create_signal("Destroying"),
            attribute_changed: create_signal("AttributeChanged"),
            part_data: None,
            humanoid_data: None,
            player_data: None,
            model_data: None,
            gui_data: None,
            weld_data: None,
            billboard_gui_data: None,
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

    pub fn new_player_gui(name: &str) -> Self {
        Self::new(ClassName::PlayerGui, name)
    }

    pub fn new_screen_gui(name: &str) -> Self {
        let mut inst = Self::new(ClassName::ScreenGui, name);
        inst.gui_data = Some(GuiObjectData::new_screen_gui());
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
        let threads = destroying.fire_as_coroutines(lua, mlua::MultiValue::new())?;
        crate::game::lua::events::track_yielded_threads(lua, threads)?;

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
        new_data.tags = data.tags.clone();
        new_data.part_data = data.part_data.clone();
        new_data.humanoid_data = data.humanoid_data.clone();
        new_data.model_data = data.model_data.clone();

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

        // Tag methods (Roblox CollectionService-style tags)
        methods.add_method("AddTag", |_, this, tag: String| {
            this.add_tag(&tag);
            Ok(())
        });

        methods.add_method("HasTag", |_, this, tag: String| {
            Ok(this.has_tag(&tag))
        });

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

        methods.add_method(
            "Move",
            |_, _this, (_direction, _relative): (Vector3, Option<bool>)| Ok(()),
        );

        methods.add_method(
            "MoveTo",
            |_, this, (position, _part): (Vector3, Option<Instance>)| {
                let mut data = this.data.lock().unwrap();
                let id = data.id.0;
                if let Some(humanoid) = &mut data.humanoid_data {
                    eprintln!(
                        "[Humanoid] MoveTo id={} target=({:.2},{:.2},{:.2})",
                        id,
                        position.x,
                        position.y,
                        position.z
                    );
                    humanoid.move_to_target = Some(position);
                } else {
                    eprintln!("[Humanoid WARN] MoveTo called on non-humanoid instance");
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
            |_, (class_name, parent): (String, Option<Instance>)| -> Result<Instance> {
                let instance = match class_name.as_str() {
                    "Part" => Instance::from_data(InstanceData::new_part("Part")),
                    "Model" => Instance::from_data(InstanceData::new_model("Model")),
                    "Humanoid" => Instance::from_data(InstanceData::new_humanoid("Humanoid")),
                    "Folder" => Instance::new(ClassName::Folder, "Folder"),
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
