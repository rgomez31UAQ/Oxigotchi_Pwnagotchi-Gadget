-- uptime.lua: System uptime with "UP: HH:MM:SS" label.
plugin = {}
plugin.name    = "uptime"
plugin.version = "1.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("uptime", {
        x     = config.x,
        y     = config.y,
        font  = "small",
        label = "UP",
    })
end

-- Format as DD:HH:MM (days:hours:minutes)
local function fmt_ddhhmm(secs)
    local d = math.floor(secs / 86400)
    local h = math.floor((secs % 86400) / 3600)
    local m = math.floor((secs % 3600) / 60)
    return string.format("%02d:%02d:%02d", d, h, m)
end

function on_epoch(state)
    set_indicator("uptime", fmt_ddhhmm(state.uptime_secs))
end
