-- ao_status.lua: AO status matching Python format.
-- Format: "AO: {session}/{total} | {compact_uptime}" when running
--         "AO: off" when stopped, "AO: ERR" when failed
plugin = {}
plugin.name    = "ao_status"
plugin.version = "2.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("ao_status", {
        x    = config.x,
        y    = config.y,
        font = "small",
    })
end

-- Format seconds as compact uptime: "5s", "12m", "1h23m"
local function compact_uptime(secs)
    if secs < 60 then
        return secs .. "s"
    elseif secs < 3600 then
        return math.floor(secs / 60) .. "m"
    else
        local h = math.floor(secs / 3600)
        local m = math.floor((secs % 3600) / 60)
        if m == 0 then
            return h .. "h"
        end
        return h .. "h" .. m .. "m"
    end
end

function on_epoch(state)
    local s
    if state.ao_state == "FAILED" then
        s = "AO: ERR"
    elseif state.ao_state == "STOPPED" then
        s = "AO: off"
    else
        s = "AO: " .. state.handshakes .. "/" .. state.captures_total
            .. " | " .. compact_uptime(state.ao_uptime_secs)
            .. " | CH:" .. state.ao_channels
    end
    set_indicator("ao_status", s)
end
