local Hud = {}

local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
local Config = require(ModulesFolder:WaitForChild("Config", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))
local Enemies = require(ModulesFolder:WaitForChild("Enemies", 2))

local HUD_NAME = "ParityHud"
local TITLE_LABEL = "TitleLabel"
local STATUS_LABEL = "StatusLabel"
local STATS_LABEL = "StatsLabel"
local HINTS_LABEL = "HintsLabel"

local function getHumanoid(player)
    local character = player.Character
    if not character then
        return nil
    end
    return character:FindFirstChild("Humanoid")
end

local function ensurePlayerGui(player)
    local player_gui = player:FindFirstChild("PlayerGui")
    if player_gui then
        return player_gui
    end
    return player:WaitForChild("PlayerGui", 2)
end

local function makeLabel(name, size, position, text_size, color)
    local label = Instance.new("TextLabel")
    label.Name = name
    label.Size = size
    label.Position = position
    label.BackgroundTransparency = 1
    label.TextXAlignment = Enum.TextXAlignment.Left
    label.TextYAlignment = Enum.TextYAlignment.Top
    label.Font = Enum.Font.GothamBold
    label.TextSize = text_size
    label.TextColor3 = color
    label.TextStrokeTransparency = 0.35
    label.Text = ""
    return label
end

local function getOrCreateHud(player)
    local player_gui = ensurePlayerGui(player)
    if not player_gui then
        return nil
    end

    local existing = player_gui:FindFirstChild(HUD_NAME)
    if existing then
        return existing
    end

    local screen_gui = Instance.new("ScreenGui")
    screen_gui.Name = HUD_NAME
    screen_gui.ResetOnSpawn = false
    screen_gui.Parent = player_gui

    local panel = Instance.new("Frame")
    panel.Name = "Panel"
    panel.Size = UDim2.new(0, 460, 0, 164)
    panel.Position = UDim2.new(0, 16, 0, 14)
    panel.BackgroundColor3 = Color3.new(0.06, 0.07, 0.09)
    panel.BackgroundTransparency = 0.22
    panel.BorderSizePixel = 0
    panel.Parent = screen_gui

    local corner = Instance.new("UICorner")
    corner.CornerRadius = UDim.new(0, 12)
    corner.Parent = panel

    local title = makeLabel(
        TITLE_LABEL,
        UDim2.new(1, -20, 0, 28),
        UDim2.new(0, 10, 0, 8),
        22,
        Color3.new(0.95, 0.97, 1.0)
    )
    title.Parent = panel

    local status = makeLabel(
        STATUS_LABEL,
        UDim2.new(1, -20, 0, 56),
        UDim2.new(0, 10, 0, 40),
        16,
        Color3.new(0.82, 0.92, 1.0)
    )
    status.Parent = panel

    local stats = makeLabel(
        STATS_LABEL,
        UDim2.new(1, -20, 0, 48),
        UDim2.new(0, 10, 0, 92),
        14,
        Color3.new(0.95, 0.95, 0.95)
    )
    stats.Parent = panel

    local hints = makeLabel(
        HINTS_LABEL,
        UDim2.new(1, -20, 0, 22),
        UDim2.new(0, 10, 0, 136),
        13,
        Color3.new(0.72, 0.78, 0.86)
    )
    hints.Parent = panel

    return screen_gui
end

local function getPlayerValues(player)
    local kills = tonumber(player:GetAttribute("Kills")) or 0
    local damage = tonumber(player:GetAttribute("DamageDealt")) or 0
    local shots = tonumber(player:GetAttribute("ShotsFired")) or 0
    local weapon = tostring(player:GetAttribute("CurrentWeapon") or "Rifle")

    local health = 0
    local max_health = 0
    local humanoid = getHumanoid(player)
    if humanoid then
        health = math.max(0, math.floor(humanoid.Health + 0.5))
        max_health = math.max(0, math.floor(humanoid.MaxHealth + 0.5))
    end

    return {
        kills = kills,
        damage = damage,
        shots = shots,
        weapon = weapon,
        health = health,
        max_health = max_health,
    }
end

local function formatPhaseText()
    if State.phase == "Completed" then
        return "Mission Complete"
    end
    if State.phase == "Failed" then
        return "Mission Failed"
    end
    if State.phase == "Prep" then
        return "Wave incoming. Hold your position."
    end
    if State.phase == "Intermission" then
        return "Reloading window. Next wave soon."
    end
    if State.phase == "Active" then
        return "Active combat. Clear all enemies."
    end
    return "Waiting for players."
end

function Hud.ensure(player)
    return getOrCreateHud(player)
end

function Hud.clear(player)
    local player_gui = player:FindFirstChild("PlayerGui")
    if not player_gui then
        return
    end
    local gui = player_gui:FindFirstChild(HUD_NAME)
    if gui then
        gui:Destroy()
    end
end

function Hud.update(player)
    local gui = getOrCreateHud(player)
    if not gui then
        return
    end

    local panel = gui:FindFirstChild("Panel")
    if not panel then
        return
    end

    local title = panel:FindFirstChild(TITLE_LABEL)
    local status = panel:FindFirstChild(STATUS_LABEL)
    local stats = panel:FindFirstChild(STATS_LABEL)
    local hints = panel:FindFirstChild(HINTS_LABEL)

    if not title or not status or not stats or not hints then
        return
    end

    local values = getPlayerValues(player)
    local wave_now = math.max(State.current_wave, 0)
    local wave_total = Config.WAVES.MAX_WAVES
    local alive_enemies = Enemies.aliveCount()
    local phase_text = formatPhaseText()

    title.Text = "Containment Yard - PVE Shooter"
    status.Text = "Phase: " .. tostring(State.phase)
        .. "  |  Wave: " .. tostring(wave_now) .. "/" .. tostring(wave_total)
        .. "  |  Enemies Alive: " .. tostring(alive_enemies)
        .. "\nStatus: " .. phase_text
    stats.Text = "HP: " .. tostring(values.health) .. "/" .. tostring(values.max_health)
        .. "  |  Weapon: " .. values.weapon
        .. "  |  Kills: " .. tostring(values.kills)
        .. "  |  Damage: " .. tostring(values.damage)
        .. "  |  Shots: " .. tostring(values.shots)
    hints.Text = "Inputs: send Fire(target=[x,y,z], weapon) and SetWeapon(weapon) via AgentInputService"
end

return Hud
