--[[
  Ridgeback Terminal — Snow fullscreen particle plugin
  File: assets/plugins/snow_particles.lua

  Spawns gentle snowflakes drifting down across the entire terminal viewport.
  This is a "fullscreen" trigger plugin — particles are emitted every frame
  regardless of user input.

  Copy and modify to create rain, bubbles, fireflies, floating balloons, etc.
--]]

RIDGEBACK_PARTICLE_PLUGIN = {
    id           = "snow",
    display_name = "❄️ Snow",
    trigger      = "fullscreen",
    params       = {
        {
            key     = "density",
            label   = "Snowflakes per second",
            type    = "float",
            min     = 1.0,
            max     = 100.0,
            default = 20.0,
        },
        {
            key     = "fall_speed",
            label   = "Fall speed",
            type    = "float",
            min     = 10.0,
            max     = 300.0,
            default = 60.0,
        },
        {
            key     = "sway",
            label   = "Horizontal sway",
            type    = "float",
            min     = 0.0,
            max     = 100.0,
            default = 25.0,
        },
        {
            key     = "color",
            label   = "Snowflake colour",
            type    = "color",
            default = "#ffffff",
        },
        {
            key     = "opacity",
            label   = "Opacity",
            type    = "float",
            min     = 0.0,
            max     = 1.0,
            default = 0.7,
        },
        {
            key     = "min_size",
            label   = "Min size",
            type    = "float",
            min     = 0.5,
            max     = 6.0,
            default = 1.0,
        },
        {
            key     = "max_size",
            label   = "Max size",
            type    = "float",
            min     = 1.0,
            max     = 10.0,
            default = 3.5,
        },
    },
}

-- Simple pseudo-RNG
local rng_state = 42
local function seed_rng(extra)
    rng_state = (rng_state + math.floor((extra or 0) * 100000)) % 2147483647
    if rng_state == 0 then rng_state = 1 end
end
local function rand()
    rng_state = (rng_state * 1103515245 + 12345) % 2147483648
    return rng_state / 2147483648
end

-- Parse "#RRGGBB" hex string to r, g, b (0-1 floats)
local function hex_to_rgb(hex)
    if not hex or #hex < 7 then return 1, 1, 1 end
    hex = hex:sub(2)
    local r = tonumber(hex:sub(1,2), 16) / 255
    local g = tonumber(hex:sub(3,4), 16) / 255
    local b = tonumber(hex:sub(5,6), 16) / 255
    return r or 1, g or 1, b or 1
end

function on_frame(dt, width, height, params)
    local density    = params.density or 20
    local fall_speed = params.fall_speed or 60
    local sway       = params.sway or 25
    local hex        = params.color or "#ffffff"
    local opacity    = params.opacity or 0.7
    local min_size   = params.min_size or 1.0
    local max_size   = params.max_size or 3.5

    -- Seed RNG with dt fractional bits for variation between frames
    seed_rng(dt * 1000 + width)

    local cr, cg, cb = hex_to_rgb(hex)

    -- Probabilistic spawning: expected count = density * dt.
    -- Integer part always spawns, fractional part spawns with that probability.
    local expected = density * dt
    local to_spawn = math.floor(expected)
    local frac = expected - to_spawn
    if rand() < frac then
        to_spawn = to_spawn + 1
    end

    local particles = {}
    for i = 1, to_spawn do
        local sz = min_size + rand() * (max_size - min_size)
        particles[#particles + 1] = {
            x       = rand() * width,
            y       = -sz * 2,  -- slightly above viewport
            vx      = (rand() - 0.5) * sway,
            vy      = fall_speed * (0.7 + rand() * 0.6),
            life    = (height / (fall_speed * 0.5)) + 4.0,  -- generous lifetime
            radius  = sz,
            color   = {
                r = cr,
                g = cg,
                b = cb,
                a = opacity * (0.5 + rand() * 0.5),
            },
            gravity = 0.0,   -- constant drift, no acceleration
            drag    = 0.0,   -- no drag for smooth fall
        }
    end

    return particles
end

