xhost +si:localuser:root
sudo env QT_QPA_PLATFORM=wayland XAUTHORITY=/home/kidofcubes/.Xauthority XDG_RUNTIME_DIR=/run/user/1000 DISPLAY=:0 XDG_SESSION_TYPE=wayland XDG_SESSION_ID=1 WAYLAND_DISPLAY=wayland-1 ngfx-ui
