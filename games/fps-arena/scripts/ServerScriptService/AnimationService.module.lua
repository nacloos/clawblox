local Players = game:GetService("Players")

local AnimationService = {}

local playerStates = {}
local TRACK_CONFIG = {
    idle = {
        animation_id = "local://idle_default",
        looped = true,
        priority = 0,
        weight = 0.75,
        speed = 1.0,
        fade_in = 0.15,
        fade_out = 0.12,
    },
    walk = {
        animation_id = "local://walk_default",
        looped = true,
        priority = 1,
        weight = 1.0,
        speed = 1.0,
        fade_in = 0.12,
        fade_out = 0.1,
    },
    fire = {
        priority = 3,
        weight = 1.0,
        speed = 1.0,
        fade_in = 0.02,
        fade_out = 0.08,
    },
    reload = {
        priority = 2,
        weight = 1.0,
        speed = 1.0,
        fade_in = 0.06,
        fade_out = 0.06,
    },
}
local WALK_START_SPEED = 1.2
local WALK_STOP_SPEED = 0.7

local function getHumanoid(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("Humanoid")
end

local function fireAnimationIdForWeapon(weaponId)
    if weaponId == 3 then
        return "local://fire_shotgun"
    end
    return "local://fire_rifle"
end

local function reloadAnimationIdForWeapon(weaponId)
    if weaponId == 3 then
        return "local://reload_shotgun"
    end
    return "local://reload_rifle"
end

local function getState(player)
    return player and playerStates[player.UserId] or nil
end

local function loadTrackForPlayer(player, animationId, priority)
    local pstate = getState(player)
    if not pstate or not pstate.humanoid then
        return nil
    end

    local existing = pstate.tracks[animationId]
    if existing then
        return existing
    end

    local animation = Instance.new("Animation")
    animation.AnimationId = animationId
    local ok, track = pcall(function()
        return pstate.humanoid:LoadAnimation(animation)
    end)
    if not ok or not track then
        return nil
    end

    if type(priority) == "number" then
        pcall(function()
            track.Priority = priority
        end)
    end

    pstate.tracks[animationId] = track
    return track
end

local function playTrack(player, animationId, opts)
    local looped = opts and opts.looped or false
    local priority = opts and opts.priority or nil
    local fadeIn = opts and opts.fade_in or nil
    local weight = opts and opts.weight or nil
    local speed = opts and opts.speed or nil

    local track = loadTrackForPlayer(player, animationId, priority)
    if not track then
        return
    end
    pcall(function()
        if type(priority) == "number" then
            track.Priority = priority
        end
        track.Looped = looped
        track:Play(fadeIn, weight, speed)
    end)
end

local function stopTrack(player, animationId, fadeOut)
    local pstate = getState(player)
    local track = pstate and pstate.tracks[animationId] or nil
    if not track then
        return
    end
    pcall(function()
        track:Stop(fadeOut)
    end)
end

function AnimationService.Init()
end

function AnimationService.BindCharacter(player, character)
    local humanoid = getHumanoid(character)
    if not player or not humanoid then
        return
    end

    AnimationService.StopAll(player)

    playerStates[player.UserId] = {
        character = character,
        humanoid = humanoid,
        tracks = {},
        locomotion = "idle",
    }

    playTrack(player, TRACK_CONFIG.idle.animation_id, TRACK_CONFIG.idle)
end

function AnimationService.UnbindPlayer(player)
    if not player then
        return
    end
    AnimationService.StopAll(player)
    playerStates[player.UserId] = nil
end

function AnimationService.StopAll(player)
    local pstate = getState(player)
    if not pstate then
        return
    end
    for _, track in pairs(pstate.tracks) do
        pcall(function()
            track:Stop(0)
        end)
    end
end

local function setLocomotionState(player, pstate, locomotion)
    if pstate.locomotion == locomotion then
        return
    end

    if locomotion == "walk" then
        playTrack(player, TRACK_CONFIG.walk.animation_id, TRACK_CONFIG.walk)
        stopTrack(player, TRACK_CONFIG.idle.animation_id, TRACK_CONFIG.idle.fade_out)
    else
        stopTrack(player, TRACK_CONFIG.walk.animation_id, TRACK_CONFIG.walk.fade_out)
        playTrack(player, TRACK_CONFIG.idle.animation_id, TRACK_CONFIG.idle)
    end

    pstate.locomotion = locomotion
end

local function updateLocomotionForPlayer(player, pstate)
    local character = player and player.Character or nil
    if not character then
        return
    end
    local root = character:FindFirstChild("HumanoidRootPart")
    local humanoid = character:FindFirstChild("Humanoid")
    if not root or not humanoid or humanoid.Health <= 0 then
        setLocomotionState(player, pstate, "idle")
        return
    end

    local v = root.Velocity
    local horizontalSpeed = math.sqrt(v.X * v.X + v.Z * v.Z)
    if pstate.locomotion == "walk" then
        if horizontalSpeed <= WALK_STOP_SPEED then
            setLocomotionState(player, pstate, "idle")
        end
        return
    end
    if horizontalSpeed >= WALK_START_SPEED then
        setLocomotionState(player, pstate, "walk")
    end
end

local function stopFire(player)
    stopTrack(player, "local://fire_rifle", TRACK_CONFIG.fire.fade_out)
    stopTrack(player, "local://fire_shotgun", TRACK_CONFIG.fire.fade_out)
end

function AnimationService.PlayFire(player, weaponId)
    if not player then
        return
    end
    local fireId = fireAnimationIdForWeapon(weaponId)
    local reloadId = reloadAnimationIdForWeapon(weaponId)
    stopTrack(player, reloadId, TRACK_CONFIG.reload.fade_out)
    playTrack(player, fireId, {
        looped = false,
        priority = TRACK_CONFIG.fire.priority,
        weight = TRACK_CONFIG.fire.weight,
        speed = TRACK_CONFIG.fire.speed,
        fade_in = TRACK_CONFIG.fire.fade_in,
    })
end

function AnimationService.PlayReload(player, weaponId)
    if not player then
        return
    end
    local reloadId = reloadAnimationIdForWeapon(weaponId)
    stopFire(player)
    playTrack(player, reloadId, {
        looped = false,
        priority = TRACK_CONFIG.reload.priority,
        weight = TRACK_CONFIG.reload.weight,
        speed = TRACK_CONFIG.reload.speed,
        fade_in = TRACK_CONFIG.reload.fade_in,
    })
end

function AnimationService.StopReload(player, weaponId)
    if not player then
        return
    end
    if type(weaponId) == "number" then
        stopTrack(player, reloadAnimationIdForWeapon(weaponId), TRACK_CONFIG.reload.fade_out)
        return
    end
    stopTrack(player, "local://reload_rifle", TRACK_CONFIG.reload.fade_out)
    stopTrack(player, "local://reload_shotgun", TRACK_CONFIG.reload.fade_out)
end

function AnimationService.Tick()
    for _, player in ipairs(Players:GetPlayers()) do
        local pstate = getState(player)
        if pstate then
            updateLocomotionForPlayer(player, pstate)
        end
    end
end

return AnimationService
