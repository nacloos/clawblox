local Config = {
    MAP_SCALE = 2.7778,
    MAP_SIZE = 60,
    ARENA_HEIGHT = 4,

    PLAYER_MAX_HEALTH = 100,
    RESPAWN_DELAY = 2.0,

    MATCH_DURATION = 300,
    KILL_LIMIT = 20,

    DEFAULT_WEAPON_ID = 2,
    WEAPONS = {
        {
            id = 1,
            name = "Pistol",
            mag_size = 12,
            reserve = 60,
            fire_rate = 0.25,
            damage = 25,
            spread = 0.015,
            reload_time = 1.5,
            pellets = 1,
            kill_score = 100,
        },
        {
            id = 2,
            name = "Assault Rifle",
            mag_size = 30,
            reserve = 120,
            fire_rate = 0.09,
            damage = 18,
            spread = 0.025,
            reload_time = 2.0,
            pellets = 1,
            kill_score = 100,
        },
        {
            id = 3,
            name = "Shotgun",
            mag_size = 8,
            reserve = 32,
            fire_rate = 0.7,
            damage = 12,
            spread = 0.08,
            reload_time = 2.5,
            pellets = 8,
            kill_score = 100,
        },
    },

    FIRE_RANGE = 100,

    SPAWN_CLEAR_RADIUS = 4,
}

return Config
