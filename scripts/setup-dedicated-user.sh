#!/bin/sh
# setup-dedicated-user.sh — run the aural engine as a dedicated system user
# instead of your own account, so only the `aural` binary (as the `aural`
# user) can read /dev/input — nothing running as you can.
#
# What it installs (all idempotent; rerun to update the binary):
#   1. system user `aural` (no shell, no groups; home /var/lib/aural)
#   2. udev rule granting the `aural` user read access to KEYBOARD event
#      nodes only (mice/touchpads/other HID stay out of reach even for aural)
#   3. /usr/local/lib/aural/pipewire-acl.sh — waits for your session's
#      pipewire socket and grants the `aural` user traverse (x) on your
#      runtime dir + rw on the socket (the sockets are already world-rw, so
#      this is audio-only exposure)
#   4. systemd system unit `aural.service` (hardened) running `aural run`
#      as that user; its ExecStartPre (as root) applies the audio ACL, and a
#      per-login user unit re-applies it (e.g. after logout/login)
#   5. AURAL_CONFIG_DIR=/var/lib/aural for your session (environment.d +
#      profile.d) so your CLI (mute/volume/status) and the tray (--no-engine)
#      control the same daemon state; you are added to the `aural` group
#
# Usage:
#   sudo ./scripts/setup-dedicated-user.sh [path-to-aural-binary]
#   sudo ./scripts/setup-dedicated-user.sh --uninstall
#   ./scripts/setup-dedicated-user.sh --verify     (as yourself, after relogin)
#
# After setup: log out & back in (group change + env), then
# `systemctl status aural` and type anywhere.
#
# Undo everything:  sudo ./scripts/setup-dedicated-user.sh --uninstall
# Undo the old input-group grant (if you had added it):
#   sudo gpasswd -d $USER input   (then log out & back in)

set -eu

# --- part 2: verification (run as yourself after (re)login; no root) ---
if [ "${1:-}" = "--verify" ]; then
    pass=0
    fail=0
    check() {
        if eval "$2" >/dev/null 2>&1; then
            echo "  ok    $1"
            pass=$((pass + 1))
        else
            echo "  FAIL  $1"
            fail=$((fail + 1))
        fi
    }
    echo "aural dedicated-user verification (as $(id -un)):"
    check "daemon service is active" "systemctl is-active --quiet aural.service"
    check "daemon runs as the aural user" "ps -o user= -C aural | grep -qx aural"
    check "audio ACL grants aural on your runtime dir" \
        "getfacl /run/user/$(id -u) 2>/dev/null | grep -q 'user:aural'"
    check "per-login ACL re-grant unit is enabled" \
        "systemctl --user is-enabled aural-pipewire-acl.service"
    check "your session is OUT of the input group" "! id -nG | grep -qw input"
    check "your session is IN the aural group" "id -nG | grep -qw aural"
    check "AURAL_CONFIG_DIR is set in this session" "[ -n \"\${AURAL_CONFIG_DIR:-}\" ]"
    check "shared state dir is writable by you" "test -w /var/lib/aural"
    if [ "$fail" -eq 0 ]; then
        echo "all checks passed — type anywhere; tray (optional): aural menubar --no-engine"
    else
        echo "$fail check(s) failed — the failing line names the gap; see README"
        echo "(Dedicated-user mode). Group/env changes need a fresh session."
        exit 1
    fi
    exit 0
fi

[ "$(id -u)" -eq 0 ] || { echo "run with sudo: sudo $0 [binary|--uninstall]  (or run --verify as yourself)" >&2; exit 1; }

SUDO_USER_NAME="${SUDO_USER:-}"
[ -n "$SUDO_USER_NAME" ] && [ "$SUDO_USER_NAME" != root ] || {
    echo "must be invoked via sudo from your own account (SUDO_USER unset)" >&2
    exit 1
}
SUDO_UID="$(id -u "$SUDO_USER_NAME")"

SERVICE=/etc/systemd/system/aural.service
UDEV_RULE=/etc/udev/rules.d/70-aural-input.rules
ACL_HELPER=/usr/local/lib/aural/pipewire-acl.sh
USER_UNIT_DIR="/home/$SUDO_USER_NAME/.config/systemd/user"
USER_UNIT="$USER_UNIT_DIR/aural-pipewire-acl.service"
ENV_D=/etc/environment.d/50-aural.conf
PROFILE_D=/etc/profile.d/aural.sh
STATE=/var/lib/aural
INSTALL_BIN=/usr/local/bin/aural

if [ "${1:-}" = "--uninstall" ]; then
    echo "==> stopping and disabling aural.service"
    systemctl disable --now aural.service 2>/dev/null || true
    echo "==> disabling user ACL re-grant unit for $SUDO_USER_NAME"
    runuser -u "$SUDO_USER_NAME" -- systemctl --user disable --now aural-pipewire-acl.service 2>/dev/null || true
    rm -f "$USER_UNIT"
    runuser -u "$SUDO_USER_NAME" -- systemctl --user daemon-reload 2>/dev/null || true
    echo "==> revoking runtime-dir ACL (best effort)"
    setfacl -x u:aural "/run/user/$SUDO_UID" 2>/dev/null || true
    echo "==> removing unit, rule, helper, env files, binary, tray autostart"
    rm -f "$UDEV_RULE" "$SERVICE" "$ACL_HELPER" "$ENV_D" "$PROFILE_D" \
        "/home/$SUDO_USER_NAME/.config/autostart/aural-tray.desktop" /usr/local/bin/aural
    rm -rf /usr/local/lib/aural
    udevadm control --reload 2>/dev/null || true
    echo "==> removing group membership, user, state"
    gpasswd -d "$SUDO_USER_NAME" aural 2>/dev/null || true
    userdel aural 2>/dev/null || true
    rm -rf "$STATE"
    echo "done. log out & back in to drop the aural group from your session."
    exit 0
fi

find_binary() {
    # 1. explicit argument; 2. root PATH; 3. the invoking user's cargo bin;
    # 4. the invoking user's cargo target dir (CARGO_TARGET_DIR is invisible
    # to root, but ~/.cargo/target is its default); 5. the repo's own target.
    if [ -n "${1:-}" ] && [ -x "${1:-}" ]; then
        printf '%s\n' "$1"
        return 0
    fi
    for candidate in \
        "$(command -v aural 2>/dev/null || true)" \
        "/home/$SUDO_USER_NAME/.cargo/bin/aural" \
        "/home/$SUDO_USER_NAME/.cargo/target/release/aural" \
        "$(cd "$(dirname "$0")/.." 2>/dev/null && pwd)/target/release/aural"
    do
        if [ -n "$candidate" ] && [ -x "$candidate" ]; then
            # skip the install target itself: on reruns it is on root PATH,
            # but installing it onto itself fails and it hides newer builds
            [ "$(readlink -f "$candidate")" = "$INSTALL_BIN" ] && continue
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

if [ "${1:-}" != "--uninstall" ]; then
    if ! BIN="$(find_binary "${1:-}")"; then
        echo "could not find an aural binary. Searched:" >&2
        echo "  command -v aural, /home/$SUDO_USER_NAME/.cargo/bin/aural," >&2
        echo "  /home/$SUDO_USER_NAME/.cargo/target/release/aural, <repo>/target/release/aural" >&2
        echo "build one first (cargo build --release) and pass its path:" >&2
        echo "  sudo $0 /path/to/aural" >&2
        exit 1
    fi
fi
BIN="$(readlink -f "$BIN")"

echo "==> creating system user 'aural' (no shell, no groups)"
getent passwd aural >/dev/null || useradd --system --shell /usr/sbin/nologin --home-dir "$STATE" --create-home aural
getent group aural >/dev/null || groupadd --system aural

echo "==> state dir $STATE (aural:aural, group-writable for your CLI)"
install -d -o aural -g aural -m 2775 "$STATE"
usermod -aG aural "$SUDO_USER_NAME"
chown -R aural:aural "$STATE"

echo "==> installing binary to $INSTALL_BIN"
if [ "$BIN" != "$INSTALL_BIN" ]; then
    install -o root -g root -m 755 "$BIN" "$INSTALL_BIN"
else
    echo "    (source is the install target — leaving it in place)"
fi

echo "==> audio bridge helper: $ACL_HELPER"
install -d -o root -g root -m 755 /usr/local/lib/aural
cat > "$ACL_HELPER" <<'EOF'
#!/bin/sh
# pipewire-acl.sh USER UID [TIMEOUT_S]
# Wait for the user session's pipewire socket, then grant USER traverse (x)
# on the runtime dir and rw on the socket. The pipewire sockets are created
# world-rw, so this grants audio access only. Exits 0 even when the socket
# never appears (no session yet) — the systemd unit restarts and retries.
set -u
AURAL_USER="${1:?usage: pipewire-acl.sh USER UID [TIMEOUT_S]}"
RUN_USER="${2:?usage: pipewire-acl.sh USER UID [TIMEOUT_S]}"
TIMEOUT="${3:-90}"
RUNDIR="/run/user/$RUN_USER"
i=0
while [ "$i" -lt "$TIMEOUT" ] && [ ! -S "$RUNDIR/pipewire-0" ]; do
    i=$((i + 1))
    sleep 1
done
[ -S "$RUNDIR/pipewire-0" ] || exit 0
setfacl -m "u:$AURAL_USER:x" "$RUNDIR" 2>/dev/null || true
setfacl -m "u:$AURAL_USER:rw" "$RUNDIR/pipewire-0" 2>/dev/null || true
exit 0
EOF
chmod 755 "$ACL_HELPER"

echo "==> udev rule: keyboard event nodes readable by user 'aural' only"
cat > "$UDEV_RULE" <<'EOF'
# aural: let the dedicated `aural` user read keyboard event nodes only.
# Everything running as your own account stays locked out of /dev/input.
ACTION=="add|change", SUBSYSTEM=="input", ENV{ID_INPUT_KEYBOARD}=="1", \
  RUN+="/usr/bin/setfacl -m u:aural:rw $devnode"
EOF
udevadm control --reload
udevadm trigger --subsystem-match=input --action=change 2>/dev/null || true

echo "==> systemd unit $SERVICE (runs `aural run` as the aural user)"
cat > "$SERVICE" <<EOF
[Unit]
Description=aural — system-wide melodic keyboard sounds (dedicated user)
Documentation=https://github.com/bevry-vibes/aural-keyboard
After=sound.target

[Service]
Type=simple
User=aural
Group=aural
UMask=0002
Environment=AURAL_CONFIG_DIR=$STATE
Environment=XDG_RUNTIME_DIR=/run/user/$SUDO_UID
# Grant the audio ACL as root, waiting for the session's pipewire socket
# (it appears at first login; + keeps ExecStartPre privileged).
ExecStartPre=+$ACL_HELPER aural $SUDO_UID 90
ExecStart=/usr/local/bin/aural run
Restart=on-failure
RestartSec=15
TimeoutStartSec=120

# hardening: the engine needs only its state dir, input devices, and audio
NoNewPrivileges=yes
ProtectSystem=strict
ReadWritePaths=$STATE
# read-only (not "yes"): "yes" would hide /run/user entirely — including the
# pipewire socket the daemon must connect to (XDG_RUNTIME_DIR above).
ProtectHome=read-only
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
ProtectClock=yes
RestrictSUIDSGID=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable aural.service >/dev/null
# restart (not just start): applies the freshly installed binary and unit
# deterministically, whether or not a previous instance is running
systemctl restart aural.service
sleep 2
if systemctl is-active --quiet aural.service; then
    echo "==> aural.service is running"
else
    echo "==> NOTE: aural.service is not active yet (it retries every 15 s)"
    echo "    inspect with: journalctl -u aural -n 30 --no-pager"
fi

echo "==> session env: AURAL_CONFIG_DIR for your CLI and tray"
install -d -m 755 /etc/environment.d
install -d -m 755 /etc/profile.d
cat > "$ENV_D" <<EOF
# aural dedicated-user mode: CLI/tray share the daemon's state dir
AURAL_CONFIG_DIR=$STATE
EOF
cat > "$PROFILE_D" <<EOF
# aural dedicated-user mode: CLI/tray share the daemon's state dir
export AURAL_CONFIG_DIR=$STATE
EOF

echo "==> tray autostart for $SUDO_USER_NAME (aural menubar --no-engine)"
TRAY_DESKTOP_DIR="/home/$SUDO_USER_NAME/.config/autostart"
install -d -o "$SUDO_USER_NAME" -g "$SUDO_USER_NAME" "$TRAY_DESKTOP_DIR"
cat > "$TRAY_DESKTOP_DIR/aural-tray.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=aural tray
Comment=System-wide melodic keyboard sounds — tray control surface
Exec=$INSTALL_BIN menubar --no-engine
Terminal=false
X-GNOME-Autostart-enabled=true
Categories=Utility;Audio;
EOF
chown "$SUDO_USER_NAME:$SUDO_USER_NAME" "$TRAY_DESKTOP_DIR/aural-tray.desktop"

echo "==> per-login ACL re-grant for $SUDO_USER_NAME (logout/login cycles)"
install -d -o "$SUDO_USER_NAME" -g "$SUDO_USER_NAME" "$USER_UNIT_DIR"
cat > "$USER_UNIT" <<EOF
[Unit]
Description=aural dedicated-user mode: re-grant user 'aural' audio-socket access

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=$ACL_HELPER aural 30

[Install]
WantedBy=default.target
EOF
chown "$SUDO_USER_NAME:$SUDO_USER_NAME" "$USER_UNIT"
# Under sudo there is no user bus via local transport; --machine connects to
# the invoking user's systemd --user instance through its bus.
systemctl --user --machine="$SUDO_USER_NAME@.host" daemon-reload 2>/dev/null || \
    echo "  NOTE: could not daemon-reload the user manager (log in and run: systemctl --user daemon-reload)"
systemctl --user --machine="$SUDO_USER_NAME@.host" enable --now aural-pipewire-acl.service 2>/dev/null || \
    echo "  NOTE: could not enable the per-login re-grant unit (enable it after login: systemctl --user enable --now aural-pipewire-acl.service)"

echo
echo "installed. what works RIGHT NOW (no logout needed):"
echo "  - sound: the daemon is running as the aural user (systemctl status aural)"
echo "  - mute hotkey Ctrl+Shift+F12 (the daemon toggles its own shared config)"
echo "needs a LOG OUT & BACK IN (fresh session picks up groups + env):"
echo "  - your CLI control: aural mute/volume/status (aural group + AURAL_CONFIG_DIR)"
echo "  - tray control surface: aural menubar --no-engine"
echo "  - input isolation for your account: undo the old grant if not done yet:"
echo "      sudo gpasswd -d $SUDO_USER_NAME input"
echo "then verify everything end to end (as yourself, no sudo):"
echo "  ./scripts/setup-dedicated-user.sh --verify"
echo "if sound is not playing, inspect: journalctl -u aural -n 30 --no-pager"
