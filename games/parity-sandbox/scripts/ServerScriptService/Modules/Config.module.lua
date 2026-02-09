local Config = {}

Config.MARKERS = {
    Boot = "BootMarker",
    Loop = "LoopMarker",
    Wait = "WaitMarker",
    PlayerStats = "PlayerStatsMarker",
    Round = "RoundMarker",
    Combat = "CombatMarker",
    Wave = "WaveMarker",
    Perf = "PerfMarker",
}

Config.SPAWN = {
    Position = Vector3.new(-8, 6, 0),
}

Config.WAVES = {
    MAX_WAVES = 5,
    PREP_TIME = 1.0,
    INTERMISSION_TIME = 1.25,
    BASE_ENEMIES = 6,
    ENEMY_INCREMENT = 2,
    BASE_HEALTH = 40,
    HEALTH_INCREMENT = 12,
}

Config.ENEMIES = {
    SPEED = 7,
    CONTACT_RANGE = 3.0,
    CONTACT_DAMAGE = 10,
    CONTACT_COOLDOWN = 1.0,
    DESTROY_DELAY = 0.05,
    SPAWN_POINTS = {
        Vector3.new(-46, 4, -32),
        Vector3.new(46, 4, -32),
        Vector3.new(-46, 4, 32),
        Vector3.new(46, 4, 32),
    },
}

Config.WEAPONS = {
    Rifle = {
        damage = 20,
        range = 220,
        spread_deg = 1.2,
        cooldown = 0.18,
        pellets = 1,
    },
    Shotgun = {
        damage = 8,
        range = 90,
        spread_deg = 9.0,
        cooldown = 0.8,
        pellets = 6,
    },
}

Config.COMBAT = {
    GLOBAL_FIRE_CAP_PER_SEC = 12,
    LOOP_MARKER_EVERY_TICKS = 12,
}

Config.VISUALS = {
    BASE_OFFSET = Vector3.new(0.9, 1.0, -0.45),
    RECOIL_DISTANCE = 0.35,
    RECOIL_DECAY_PER_SEC = 6.5,
    FLASH_DURATION = 0.06,
}

return Config
