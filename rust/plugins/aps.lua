-- aps.lua: Access points seen counter.
plugin = {}
plugin.name    = "aps"
plugin.version = "1.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("aps", {
        x    = config.x,
        y    = config.y,
        font = "small",
        modes = {"RAGE", "SAFE"},
    })
end

function on_epoch(state)
    set_indicator("aps", "APs:" .. state.aps_seen)
end
