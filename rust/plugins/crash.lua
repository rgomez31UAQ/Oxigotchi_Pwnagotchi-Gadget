-- crash.lua: AO crash counter display.
plugin = {}
plugin.name    = "crash"
plugin.version = "1.0.0"
plugin.author  = "oxigotchi"
plugin.tag     = "default"

function on_load(config)
    register_indicator("crash", {
        x    = config.x,
        y    = config.y,
        font = "small",
        modes = {"RAGE", "SAFE"},
    })
end

function on_epoch(state)
    set_indicator("crash", "CRASH:" .. state.ao_crash_count)
end
