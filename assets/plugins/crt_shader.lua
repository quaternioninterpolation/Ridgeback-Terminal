--[[
  Ridgeback Terminal — built-in CRT shader plugin
  File: assets/plugins/crt_shader.lua

  Registers the CRT retro-monitor post-process effect.
  Users can copy this file to create custom CRT variants.
--]]

local params = {
    {
        key     = "scanline_intensity",
        label   = "Scanline intensity",
        type    = "float",
        min     = 0.0,
        max     = 4.0,
        default = 1.0,
    },
    {
        key     = "curvature",
        label   = "Screen curvature / vignette",
        type    = "float",
        min     = 0.0,
        max     = 1.0,
        default = 0.0,
    },
    {
        key     = "bloom_strength",
        label   = "Phosphor bloom",
        type    = "float",
        min     = 0.0,
        max     = 1.0,
        default = 0.0,
    },
}

ridgeback.register_shader("crt", "📺 CRT", nil, params)

