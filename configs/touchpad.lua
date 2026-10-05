-- Merge into your existing Omarchy/Hyprland Lua input configuration.
-- See docs/trackpad.md: the libinput keyboard classification fix is separate.
hl.config({
  input = {
    touchpad = {
      disable_while_typing = true,
      tap_to_click = false,
      tap_and_drag = false,
      drag_lock = 0,
    },
  },
})
