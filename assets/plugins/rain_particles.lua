--[[
  Ridgeback Terminal — Rain fullscreen particle plugin
  File: assets/plugins/rain_particles.lua

  Spawns rain streaks falling from the top with splash bursts and a rising
  flood layer at the bottom of the viewport.

  Trigger: fullscreen (emits every frame).
--]]

RIDGEBACK_PARTICLE_PLUGIN = {
    id           = "rain",
    display_name = "🌧️ Rain",
    trigger      = "fullscreen",
    params       = {
        {
            key     = "density",
            label   = "Drops per second",
            type    = "float",
            min     = 5.0,
            max     = 200.0,
            default = 60.0,
        },
        {
            key     = "fall_speed",
            label   = "Fall speed",
            type    = "float",
            min     = 100.0,
            max     = 1200.0,
            default = 500.0,
        },
        {
            key     = "wind",
            label   = "Wind (horizontal)",
            type    = "float",
            min     = -200.0,
            max     = 200.0,
            default = 30.0,
        },
        {
            key     = "drop_color",
            label   = "Rain colour",
            type    = "color",
            default = "#8ab4f8",
        },
        {
            key     = "splash_color",
            label   = "Splash colour",
            type    = "color",
            default = "#a0c8ff",
        },
        {
            key     = "flood_color",
            label   = "Flood colour",
            type    = "color",
            default = "#3a6ea5",
        },
        {
            key     = "opacity",
            label   = "Drop opacity",
            type    = "float",
            min     = 0.0,
            max     = 1.0,
            default = 0.55,
        },
        {
            key     = "splash_intensity",
            label   = "Splash intensity",
            type    = "float",
            min     = 0.0,
            max     = 3.0,
            default = 1.0,
        },
        {
            key     = "flood_opacity",
            label   = "Flood opacity",
            type    = "float",
            min     = 0.0,
            max     = 0.6,
            default = 0.18,
        },
    },
}

-- Pseudo-RNG
local rng_state = 7
local function seed_rng(extra)
    rng_state = (rng_state + math.floor((extra or 0) * 100000)) % 2147483647
    if rng_state == 0 then rng_state = 1 end
end
local function rand()
    rng_state = (rng_state * 1103515245 + 12345) % 2147483648
    return rng_state / 2147483648
end

local function hex_to_rgb(hex)
    if not hex or #hex < 7 then return 0.5, 0.7, 1.0 end
    hex = hex:sub(2)
    local r = tonumber(hex:sub(1,2), 16) / 255
    local g = tonumber(hex:sub(3,4), 16) / 255
    local b = tonumber(hex:sub(5,6), 16) / 255
    return r or 0.5, g or 0.7, b or 1.0
end

function on_frame(dt, width, height, params)
    local density         = params.density or 60
    local fall_speed      = params.fall_speed or 500
    local wind            = params.wind or 30
    local drop_hex        = params.drop_color or "#8ab4f8"
    local splash_hex      = params.splash_color or "#a0c8ff"
    local flood_hex       = params.flood_color or "#3a6ea5"
    local opacity         = params.opacity or 0.55
    local splash_int      = params.splash_intensity or 1.0
    local flood_opacity   = params.flood_opacity or 0.18

    seed_rng(dt * 1000 + width * 0.01)

    local dr, dg, db = hex_to_rgb(drop_hex)
    local sr, sg, sb = hex_to_rgb(splash_hex)
    local fr, fg, fb = hex_to_rgb(flood_hex)

    -- Probabilistic spawning
    local expected = density * dt
    local to_spawn = math.floor(expected)
    if rand() < (expected - to_spawn) then to_spawn = to_spawn + 1 end

    local particles = {}

    -- ── Rain drops ────────────────────────────────────────────────────
    for i = 1, to_spawn do
        local speed = fall_speed * (0.8 + rand() * 0.4)
        local x = rand() * (width + math.abs(wind) * 2) - math.abs(wind)
        particles[#particles + 1] = {
            x       = x,
            y       = -(rand() * 40),        -- start above viewport
            vx      = wind + (rand() - 0.5) * 20,
            vy      = speed,
            life    = (height / (speed * 0.6)) + 1.0,
            radius  = 1.0 + rand() * 0.8,    -- thin drops
            color   = {
                r = dr + (rand() - 0.5) * 0.05,
                g = dg + (rand() - 0.5) * 0.05,
                b = db + (rand() - 0.5) * 0.05,
                a = opacity * (0.6 + rand() * 0.4),
            },
            gravity = 0.0,   -- constant speed, no acceleration
            drag    = 0.0,
        }
    end

    -- ── Splash particles at the bottom ────────────────────────────────
    -- Proportional to rain density: some fraction of drops "hit" each frame
    if splash_int > 0 then
        local splash_count = math.floor(density * dt * 0.4 * splash_int)
        if rand() < (density * dt * 0.4 * splash_int - splash_count) then
            splash_count = splash_count + 1
        end

        for i = 1, splash_count do
            local sx = rand() * width
            local sy = height - 2 - rand() * 4    -- near the bottom
            local spread = 20 + rand() * 30

            -- 2-4 splash droplets per impact
            local n = 2 + math.floor(rand() * 3)
            for j = 1, n do
                local angle = -math.pi * (0.15 + rand() * 0.7)   -- upward arc
                local spd = spread * (0.5 + rand() * 0.5)
                particles[#particles + 1] = {
                    x       = sx + (rand() - 0.5) * 6,
                    y       = sy,
                    vx      = math.cos(angle) * spd * (rand() > 0.5 and 1 or -1),
                    vy      = math.sin(angle) * spd,
                    life    = 0.2 + rand() * 0.25,
                    radius  = 0.8 + rand() * 1.2,
                    color   = {
                        r = sr, g = sg, b = sb,
                        a = (0.4 + rand() * 0.4) * splash_int,
                    },
                    gravity = 2.0,   -- splash droplets arc back down fast
                    drag    = 0.3,
                }
            end
        end
    end

    -- ── Flood layer at the bottom ─────────────────────────────────────
    -- Emit wide, mostly-stationary translucent blobs that settle at floor.
    -- Spawned proportional to density so the flood builds up with the rain.
    if flood_opacity > 0 then
        local flood_count = math.floor(density * dt * 0.15)
        if rand() < (density * dt * 0.15 - flood_count) then
            flood_count = flood_count + 1
        end

        for i = 1, flood_count do
            particles[#particles + 1] = {
                x       = rand() * width,
                y       = height - 1 - rand() * 3,
                vx      = (rand() - 0.5) * 8,
                vy      = 0.5 + rand() * 1.0,    -- tiny downward drift so pileup catches it
                life    = 6.0 + rand() * 4.0,     -- long-lived
                radius  = 5.0 + rand() * 8.0,     -- wide blobs for the flood sheet
                color   = {
                    r = fr, g = fg, b = fb,
                    a = flood_opacity * (0.5 + rand() * 0.5),
                },
                gravity = 0.0,
                drag    = 0.0,
            }
        end
    end

    return particles
end

