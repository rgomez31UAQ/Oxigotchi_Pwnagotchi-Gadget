-- ip_display.lua: IP address display.
-- RAGE mode: USB IP only. SAFE mode: rotates USB/BT.
plugin = {}
plugin.name    = "ip_display"
plugin.version = "2.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

local show_bt = false

function on_load(config)
    register_indicator("ip_display", {
        x    = config.x,
        y    = config.y,
        font = "small",
    })
end

function on_epoch(state)
    if state.mode == "RAGE" then
        -- RAGE: USB tether only (BT is off)
        set_indicator("ip_display", state.display_ip)
    else
        -- SAFE: rotate between USB and BT IP
        if show_bt and state.bt_connected and state.bt_ip ~= "" then
            set_indicator("ip_display", "BT:" .. state.bt_ip)
        else
            set_indicator("ip_display", state.display_ip)
        end
        show_bt = not show_bt
    end
end
