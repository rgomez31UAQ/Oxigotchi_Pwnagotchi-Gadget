-- bt_summary.lua: BT mode top-bar summary (replaces ao_status in BT mode).
-- Format: "BT: 3d | 1a | 2c | PR:ok"
plugin = {}
plugin.name    = "bt_summary"
plugin.version = "1.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("bt_summary", {
        x    = config.x,
        y    = config.y,
        font = "small",
        modes = {"BT"},
    })
end

function on_epoch(state)
    local pr = state.bt_patchram_state or "?"
    if pr == "attack" then pr = "atk"
    elseif pr == "stock" then pr = "stk"
    elseif pr == "unloaded" then pr = "--"
    elseif pr == "error" then pr = "ER"
    end

    local s = "BT: " .. state.bt_devices_seen .. "d"
        .. " | " .. state.bt_active_attacks .. "a"
        .. " | " .. state.bt_total_captures .. "c"
        .. " | PR:" .. pr
    if #s > 28 then s = s:sub(1, 28) end
    set_indicator("bt_summary", s)
end
