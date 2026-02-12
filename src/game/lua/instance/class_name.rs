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
    Animation,
    Animator,
    Script,
    ModuleScript,
    ServerScriptService,
    ReplicatedStorage,
    IntValue,
    NumberValue,
    BoolValue,
    StringValue,
    ObjectValue,
    // Constraints
    Weld,
    RemoteEvent,
    BodyVelocity,
    BodyForce,
    BodyAngularVelocity,
    Attachment,
    AlignOrientation,
    // GUI classes
    BillboardGui,
    PlayerGui,
    ScreenGui,
    Frame,
    TextLabel,
    TextButton,
    ImageLabel,
    ImageButton,
    UICorner,
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
            ClassName::Animation => "Animation",
            ClassName::Animator => "Animator",
            ClassName::Script => "Script",
            ClassName::ModuleScript => "ModuleScript",
            ClassName::ServerScriptService => "ServerScriptService",
            ClassName::ReplicatedStorage => "ReplicatedStorage",
            ClassName::IntValue => "IntValue",
            ClassName::NumberValue => "NumberValue",
            ClassName::BoolValue => "BoolValue",
            ClassName::StringValue => "StringValue",
            ClassName::ObjectValue => "ObjectValue",
            ClassName::Weld => "Weld",
            ClassName::RemoteEvent => "RemoteEvent",
            ClassName::BodyVelocity => "BodyVelocity",
            ClassName::BodyForce => "BodyForce",
            ClassName::BodyAngularVelocity => "BodyAngularVelocity",
            ClassName::Attachment => "Attachment",
            ClassName::AlignOrientation => "AlignOrientation",
            ClassName::BillboardGui => "BillboardGui",
            ClassName::PlayerGui => "PlayerGui",
            ClassName::ScreenGui => "ScreenGui",
            ClassName::Frame => "Frame",
            ClassName::TextLabel => "TextLabel",
            ClassName::TextButton => "TextButton",
            ClassName::ImageLabel => "ImageLabel",
            ClassName::ImageButton => "ImageButton",
            ClassName::UICorner => "UICorner",
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
            "Animation" => matches!(self, ClassName::Animation),
            "Animator" => matches!(self, ClassName::Animator),
            "Script" => matches!(self, ClassName::Script),
            "ModuleScript" => matches!(self, ClassName::ModuleScript),
            "ServerScriptService" => matches!(self, ClassName::ServerScriptService),
            "ReplicatedStorage" => matches!(self, ClassName::ReplicatedStorage),
            "IntValue" => matches!(self, ClassName::IntValue),
            "NumberValue" => matches!(self, ClassName::NumberValue),
            "BoolValue" => matches!(self, ClassName::BoolValue),
            "StringValue" => matches!(self, ClassName::StringValue),
            "ObjectValue" => matches!(self, ClassName::ObjectValue),
            "ValueBase" => matches!(
                self,
                ClassName::IntValue
                    | ClassName::NumberValue
                    | ClassName::BoolValue
                    | ClassName::StringValue
                    | ClassName::ObjectValue
            ),
            // Constraints
            "Constraint" => matches!(self, ClassName::Weld),
            "Weld" => matches!(self, ClassName::Weld),
            "RemoteEvent" => matches!(self, ClassName::RemoteEvent),
            "BodyVelocity" => matches!(self, ClassName::BodyVelocity),
            "BodyForce" => matches!(self, ClassName::BodyForce),
            "BodyAngularVelocity" => matches!(self, ClassName::BodyAngularVelocity),
            "Attachment" => matches!(self, ClassName::Attachment),
            "AlignOrientation" => matches!(self, ClassName::AlignOrientation),
            "BodyMover" => matches!(
                self,
                ClassName::BodyVelocity | ClassName::BodyForce | ClassName::BodyAngularVelocity
            ),
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
            "UICorner" => matches!(self, ClassName::UICorner),
            "UIComponent" => matches!(self, ClassName::UICorner),
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
