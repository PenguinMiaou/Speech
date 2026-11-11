# Layout of the macOS disk image, read by dmgbuild:
#   dmgbuild -s src-tauri/dmg/settings.py -D app=<path to Speech.app> Speech <out.dmg>
import os

here = defines.get("settings_dir", "src-tauri/dmg")  # noqa: F821
app = defines["app"]  # noqa: F821
name = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = os.path.join(here, "..", "icons", "icon.icns")
background = os.path.join(here, "background.png")

window_rect = ((200, 140), (640, 420))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False

arrange_by = None
icon_size = 128
text_size = 13
icon_locations = {name: (170, 196), "Applications": (470, 196)}
