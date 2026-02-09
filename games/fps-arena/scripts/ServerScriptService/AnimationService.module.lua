local AnimationService = {}

local playerStates = {}

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

local function loadTrackForPlayer(player, animationId)
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

    pstate.tracks[animationId] = track
    return track
end

local function playTrack(player, animationId, looped)
    local track = loadTrackForPlayer(player, animationId)
    if not track then
        return
    end
    pcall(function()
        track.Looped = looped
        track:Play()
    end)
end

local function stopTrack(player, animationId)
    local pstate = getState(player)
    local track = pstate and pstate.tracks[animationId] or nil
    if not track then
        return
    end
    pcall(function()
        track:Stop()
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
    }

    playTrack(player, "local://idle_default", true)
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
            track:Stop()
        end)
    end
end

function AnimationService.PlayFire(player, weaponId)
    if not player then
        return
    end
    local fireId = fireAnimationIdForWeapon(weaponId)
    local reloadId = reloadAnimationIdForWeapon(weaponId)
    stopTrack(player, reloadId)
    playTrack(player, fireId, false)
end

function AnimationService.PlayReload(player, weaponId)
    if not player then
        return
    end
    local reloadId = reloadAnimationIdForWeapon(weaponId)
    playTrack(player, reloadId, false)
end

function AnimationService.StopReload(player, weaponId)
    if not player then
        return
    end
    if type(weaponId) == "number" then
        stopTrack(player, reloadAnimationIdForWeapon(weaponId))
        return
    end
    stopTrack(player, "local://reload_rifle")
    stopTrack(player, "local://reload_shotgun")
end

return AnimationService
