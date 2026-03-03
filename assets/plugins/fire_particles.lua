--[[
  Ridgeback Terminal — Fire typing-particle plugin
  File: assets/plugins/fire_particles.lua

  Emits fire embers and smoke puffs on each keypress.
  Users can copy this file and modify colours, counts, physics, etc.

  Plugin contract
  ───────────────
  RIDGEBACK_PARTICLE_PLUGIN = { id, display_name, trigger, params }

  Trigger functions (implement the ones matching your trigger modes):
      on_keypress(x, y, params)               → list of particles
      on_newline(x, y, params)                 → list of particles
      on_frame(dt, width, height, params)      → list of particles

  Each particle table:
      x, y          — starting position (terminal-local pixels)
      vx, vy        — initial velocity (pixels/sec)
      life          — lifetime in seconds
      radius        — circle radius in pixels
      color         — { r, g, b, a } each 0.0–1.0  (a = opacity/transparency)
      gravity       — gravity multiplier (default 1.0; 0 = float, negative = rise)
      drag          — drag coefficient (default 0.5; higher = more air resistance)
--]]

RIDGEBACK_PARTICLE_PLUGIN = {
    id           = "fire",
    display_name = "🔥 Fire Particles",
    trigger      = "keypress",
    params       = {
        {
            key     = "particle_count",
            label   = "Embers per keypress",
            type    = "int",
            min     = 1,
            max     = 30,
            default = 8,
        },
        {
            key     = "smoke_count",
            label   = "Smoke puffs per keypress",
            type    = "int",
            min     = 0,
            max     = 15,
            default = 5,
        },
        {
            key     = "speed_scale",
            label   = "Particle speed scale",
            type    = "float",
            min     = 0.1,
            max     = 4.0,
            default = 1.0,
        },
        {
            key     = "ember_color",
            label   = "Ember colour",
            type    = "color",
            default = "#ff6600",
        },
        {
            key     = "smoke_color",
            label   = "Smoke colour",
            type    = "color",
            default = "#888888",
        },
    },
}

-- Simple seeded pseudo-RNG (PCG-style)
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
    hex = hex:sub(2) -- strip '#'
    local r = tonumber(hex:sub(1,2), 16) / 255
    local g = tonumber(hex:sub(3,4), 16) / 255
    local b = tonumber(hex:sub(5,6), 16) / 255
    return r or 1, g or 1, b or 1
end

function on_keypress(x, y, params)
    local ember_count = params.particle_count or 8
    local smoke_count = params.smoke_count or 5
    local speed       = params.speed_scale or 1.0
    local ember_hex   = params.ember_color or "#ff6600"
    local smoke_hex   = params.smoke_color or "#888888"

    seed_rng(x, y)

    local er, eg, eb = hex_to_rgb(ember_hex)
    local sr, sg, sb = hex_to_rgb(smoke_hex)

    local particles = {}

    -- Embers: burst upward with slight spread
    for i = 1, ember_count do
        local angle = rand() * 2 * math.pi
        local spd   = (20 + rand() * 60) * speed
        local heat  = 0.7 + rand() * 0.3
        -- Tint embers from deep orange to bright yellow based on heat
        local cr = er + (1.0 - er) * heat * 0.3
        local cg = eg + (1.0 - eg) * heat * 0.5
        local cb = eb * heat * 0.2
        particles[#particles + 1] = {
            x       = x,
            y       = y,
            vx      = math.cos(angle) * spd * 0.4,
            vy      = -(30 + rand() * 80) * speed,
            life    = 0.4 + rand() * 0.5,
            radius  = 2 + rand() * 3,
            color   = { r = cr, g = cg, b = cb, a = 0.9 },
            gravity = 0.6,
            drag    = 0.3,
        }
    end

    -- Smoke: slow, rising, expanding puffs
    for i = 1, smoke_count do
        particles[#particles + 1] = {
            x       = x + (rand() - 0.5) * 10,
            y       = y,
            vx      = (rand() - 0.5) * 15 * speed,
            vy      = -(10 + rand() * 25) * speed,
            life    = 0.8 + rand() * 0.8,
            radius  = 4 + rand() * 6,
            color   = { r = sr, g = sg, b = sb, a = 0.25 },
            gravity = -0.3,  -- smoke floats up
            drag    = 0.8,
        }
    end

    return particles
end

