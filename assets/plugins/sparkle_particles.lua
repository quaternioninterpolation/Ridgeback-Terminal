--[[
  Ridgeback Terminal — example custom particle plugin
  File: assets/plugins/sparkle_particles.lua

  Demonstrates how to write a completely custom particle effect.
  Drop this file (and any others) in:
      <config_dir>/ridgeback/plugins/
  then press Ctrl+Shift+P to reload without restarting.
--]]

local params = {
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
}

ridgeback.register_particles("sparkle", "✨ Sparkles", params)

