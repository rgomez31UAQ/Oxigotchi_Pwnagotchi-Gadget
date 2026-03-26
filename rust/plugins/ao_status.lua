-- ao_status.lua: AO session status on e-ink top line.
-- Format: "AO: {handshakes}/{captures} | {uptime} | CH:{channels}" when running
--   handshakes = validated captures moved to SD this session
--   captures   = pcapng files created by AO in tmpfs this session
-- "AO: off" when stopped, "AO: ERR" when failed
plugin = {}
plugin.name    = "ao_status"
plugin.version = "4.0.0"
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
        s = "AO: " .. state.session_handshakes .. "/" .. state.session_captures
            .. " | " .. compact_uptime(state.ao_uptime_secs)
            .. " | CH:" .. state.ao_channels
    end
    if #s > 18 then s = s:sub(1, 18) end
    set_indicator("ao_status", s)
end
