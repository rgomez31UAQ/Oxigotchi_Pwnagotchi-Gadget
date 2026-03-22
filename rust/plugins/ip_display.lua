-- ip_display.lua: IP address display.
-- Rust controls rotation: RAGE = USB only, SAFE = rotates USB/BT every 5s.
plugin = {}
plugin.name    = "ip_display"
plugin.version = "3.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("ip_display", {
        x    = config.x,
        y    = config.y,
        font = "small",
    })
end

function on_epoch(state)
    set_indicator("ip_display", state.display_ip)
end
