-- mode.lua: Shows current operating mode (RAGE/SAFE) with optional rage level.
plugin = {}
plugin.name    = "mode"
plugin.version = "1.2.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("mode", {
        x    = config.x,
        y    = config.y,
        font = "small",
    })
end

function on_epoch(state)
    local text = state.mode
    if state.mode == "RAGE" and state.rage_level > 0 then
        text = "RAGE:" .. state.rage_level
    elseif state.mode == "BT" then
        local lvl = state.bt_rage_level or ""
        if lvl == "Low" then text = "BT:1"
        elseif lvl == "Medium" then text = "BT:2"
        elseif lvl == "High" then text = "BT:3"
        end
    end
    set_indicator("mode", text)
end
