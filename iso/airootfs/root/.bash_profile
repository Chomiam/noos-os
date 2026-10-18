#
# noos-os live medium: start the installer fullscreen on tty1.
#
# Every other tty keeps a normal shell, so there is always an escape hatch if
# the graphical session does not come up.

if [[ $(tty) == /dev/tty1 ]]; then
  # WebKitGTK's DMABUF renderer fails on some virtual GPUs and on NVIDIA,
  # producing a blank window. The app also sets this itself; here it covers
  # anything else launched from this shell.
  export WEBKIT_DISABLE_DMABUF_RENDERER=1
  # Let cage fall back to software rendering instead of refusing to start when
  # the GPU cannot give it a GL context.
  export WLR_RENDERER_ALLOW_SOFTWARE=1

  # The compositor's own layout, used for anything outside the installer.
  #
  # wlroots asks libxkbcommon for a keymap with no explicit names, and
  # libxkbcommon falls back to these variables. They are read once, when cage
  # starts, and nothing can change them afterwards — no Wayland protocol lets a
  # client set its compositor's keymap.
  #
  # That is why the installer no longer tries. It remaps keystrokes itself, in
  # the webview, using tables generated from the system's own XKB data — so
  # changing the layout takes effect immediately, with no restart and no
  # round-trip through this tty.
  #
  # This value is therefore only a starting point, and only matters for
  # programs that are not the installer.
  export XKB_DEFAULT_LAYOUT=us
  export XKB_DEFAULT_VARIANT=

  # No `exec`: if the installer exits or crashes we want a shell to land in,
  # not a dead session.
  cage -d -s /usr/bin/noos-installer

  echo
  echo "L'installateur s'est arrêté. Shell de secours sur tty1."
  echo "Journaux :  journalctl -b -u systemd-logind"
fi
