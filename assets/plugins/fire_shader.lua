--[[
  Ridgeback Terminal — built-in Fire shader plugin
  File: assets/plugins/fire_shader.lua

  This file registers a full-screen fire background shader effect.
  Users can copy and modify this file to create custom fire variants.

  Plugin contract
  ───────────────
  A shader plugin must call:
      ridgeback.register_shader(id, display_name, wgsl_path, param_schema)

  param_schema is a list of param descriptors:
      { key, label, type, min?, max?, default }
  Supported types: "float", "int", "bool", "color", "text"
--]]

-- ── Parameter schema ──────────────────────────────────────────────────────────
-- These appear as editable controls in Settings → Profiles → Shader Effect.

local params = {
    {
        key     = "intensity",
        label   = "Intensity",
        type    = "float",
        min     = 0.0,
        max     = 2.0,
        default = 1.0,
    },
    {
        key     = "height",
        label   = "Height from bottom (fraction)",
        type    = "float",
        min     = 0.05,
        max     = 0.80,
        default = 0.05,
    },
    {
        key     = "particle_multiplier",
        label   = "Particle multiplier",
        type    = "float",
        min     = 0.0,
        max     = 5.0,
        default = 1.0,
    },
    {
        key     = "color_base",
        label   = "Base colour (cool)",
        type    = "color",
        default = "#1a0000",
    },
    {
        key     = "color_mid",
        label   = "Mid colour (hot)",
        type    = "color",
        default = "#ff4400",
    },
    {
        key     = "color_top",
        label   = "Tip colour (bright)",
        type    = "color",
        default = "#ffdd00",
    },
}

-- ── Register ──────────────────────────────────────────────────────────────────
-- The wgsl_path is resolved relative to the shaders/ directory.
-- Pass nil to use the engine's built-in egui-drawn fire (no GPU shader needed).
ridgeback.register_shader("fire", "🔥 Fire", nil, params)

