--[[
  Ridgeback Terminal — built-in Fire typing-particle plugin
  File: assets/plugins/fire_particles.lua

  Registers fire + smoke particles emitted on each keypress.
  Users can copy/modify this file to create custom particle effects.

  Plugin contract
  ───────────────
  A particle plugin must call:
      ridgeback.register_particles(id, display_name, param_schema)

  The engine calls emit(x, y, params) whenever a key is pressed.
  The Lua function ridgeback_emit(x, y, params) should return a list of
  particle tables, each with:
      x, y          — starting position (terminal-local pixels)
      vx, vy        — initial velocity (pixels/sec)
      life          — lifetime in seconds
      radius        — circle radius in pixels
      heat          — 0.0–1.0 (drives colour in fire palette)
      is_smoke      — true / false
--]]

local params = {
    {
        key     = "particle_count",
        label   = "Particles per keypress",
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
}

ridgeback.register_particles("fire_particles", "🔥 Fire Particles", params)

