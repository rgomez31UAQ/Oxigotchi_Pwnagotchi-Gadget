-- bt_devices.lua: BT device count (replaces aps indicator in BT mode).
plugin = {}
plugin.name    = "bt_devices"
plugin.version = "1.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("bt_devices", {
        x    = config.x,
        y    = config.y,
        font = "small",
        modes = {"BT"},
    })
end

function on_epoch(state)
    set_indicator("bt_devices", "DEV:" .. state.bt_devices_seen)
end
