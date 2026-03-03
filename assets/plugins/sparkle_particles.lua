--[[
  Ridgeback Terminal — Sparkle typing-particle plugin
  File: assets/plugins/sparkle_particles.lua

  Emits colourful sparkle particles in random directions on each keypress.
  Demonstrates how to write a custom particle plugin.

  Drop this file (or your own .lua) in:
      <config_dir>/ridgeback/plugins/
  then press Ctrl+Shift+P to reload without restarting.
--]]

RIDGEBACK_PARTICLE_PLUGIN = {
    id           = "sparkle",
    display_name = "✨ Sparkles",
    trigger      = "keypress",
    params       = {
        {
            key     = "count",
            label   = "Sparkles per keypress",
            type    = "int",
            min     = 1,
            max     = 40,
            default = 12,
        },
        {
            key     = "speed",
            label   = "Sparkle speed",
            type    = "float",
            min     = 10.0,
            max     = 300.0,
            default = 80.0,
        },
        {
            key     = "color",
            label   = "Sparkle colour",
            type    = "color",
            default = "#88ccff",
        },
        {
            key     = "lifetime",
            label   = "Lifetime (seconds)",
            type    = "float",
            min     = 0.1,
            max     = 3.0,
            default = 0.6,
        },
        {
            key     = "opacity",
            label   = "Opacity",
            type    = "float",
            min     = 0.0,
            max     = 1.0,
            default = 0.85,
        },
    },
}

-- Simple seeded pseudo-RNG
local rng_state = 0
local function seed_rng(x, y)
    rng_state = math.floor(x * 1664525 + y * 1013904223) % 2147483647
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

function on_keypress(x, y, params)
    local count    = params.count or 12
    local speed    = params.speed or 80
    local hex      = params.color or "#88ccff"
    local lifetime = params.lifetime or 0.6
    local opacity  = params.opacity or 0.85

    seed_rng(x, y)
    local cr, cg, cb = hex_to_rgb(hex)

    local particles = {}
    for i = 1, count do
        local angle = rand() * 2 * math.pi
        local spd   = speed * (0.5 + rand() * 0.5)
        -- Slight colour variation per particle
        local dr = (rand() - 0.5) * 0.15
        local dg = (rand() - 0.5) * 0.15
        local db = (rand() - 0.5) * 0.15
        particles[#particles + 1] = {
            x       = x + (rand() - 0.5) * 4,
            y       = y + (rand() - 0.5) * 4,
            vx      = math.cos(angle) * spd,
            vy      = math.sin(angle) * spd,
            life    = lifetime * (0.6 + rand() * 0.4),
            radius  = 1.5 + rand() * 2,
            color   = {
                r = math.max(0, math.min(1, cr + dr)),
                g = math.max(0, math.min(1, cg + dg)),
                b = math.max(0, math.min(1, cb + db)),
                a = opacity,
            },
            gravity = 0.2,
            drag    = 0.6,
        }
    end

    return particles
end

