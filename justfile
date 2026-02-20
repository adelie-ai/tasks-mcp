set shell := ["bash", "-euo", "pipefail", "-c"]

# Paths
bin_dir          := env_var_or_default("HOME", "") + "/.local/bin"
dbus_service_dir := env_var_or_default("XDG_DATA_HOME", env_var("HOME") + "/.local/share") + "/dbus-1/services"
dbus_service_src := "dbus/org.tasks.TasksMcp.service"
dbus_service_dst := dbus_service_dir + "/org.tasks.TasksMcp.service"
tasks_bin_src    := "target/release/tasks-mcp"
tasks_bin_dst    := bin_dir + "/tasks-mcp"
widget_dir       := "kde/plasmoid/org.tasks.widget"
widget_id        := "org.tasks.widget"

# List available commands
default: list

@list:
    just --list

# Build all workspace crates (release)
build:
    cargo build --release --workspace

# Build only the tasks-mcp daemon (release)
build-daemon:
    cargo build --release -p tasks-mcp

# Run tests
test:
    cargo test

# Run clippy
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Install tasks-mcp binary to ~/.local/bin
install-bin: build-daemon
    mkdir -p "{{bin_dir}}"
    cp "{{tasks_bin_src}}" "{{tasks_bin_dst}}"
    @echo "Installed tasks-mcp -> {{tasks_bin_dst}}"

# Install D-Bus activation file to ~/.local/share/dbus-1/services/ (rewrites Exec path for user-local install)
install-dbus:
    [ -f "{{dbus_service_src}}" ] || (echo "Missing D-Bus service file: {{dbus_service_src}}" >&2; exit 1)
    mkdir -p "{{dbus_service_dir}}"
    sed "s|Exec=.*|Exec={{bin_dir}}/tasks-mcp dbus|" "{{dbus_service_src}}" > "{{dbus_service_dst}}"
    @echo "Installed D-Bus activation file -> {{dbus_service_dst}}"

# Install the KDE plasmoid (first time)
widget-install:
    kpackagetool6 --type Plasma/Applet --install {{widget_dir}}

# Upgrade the KDE plasmoid after local changes
widget-refresh:
    kpackagetool6 --type Plasma/Applet --upgrade {{widget_dir}}

# Reinstall the KDE plasmoid (remove + install)
widget-reinstall:
    kpackagetool6 --type Plasma/Applet --remove {{widget_id}} || true
    kpackagetool6 --type Plasma/Applet --install {{widget_dir}}

# Hard refresh: reinstall plasmoid + restart plasmashell
widget-hard-refresh: widget-reinstall
    kquitapp6 plasmashell >/dev/null 2>&1 || pkill -TERM -x plasmashell || true
    sleep 0.5
    pgrep -x plasmashell >/dev/null && pkill -KILL -x plasmashell || true
    sleep 0.2
    nohup plasmashell --replace >/tmp/plasmashell-tasks.log 2>&1 &

# Remove the KDE plasmoid
widget-remove:
    kpackagetool6 --type Plasma/Applet --remove {{widget_id}} || true

# Install everything: tasks-mcp binary, plasmoid, D-Bus activation
install: install-bin widget-install install-dbus
    @echo "All components installed."

# Run the widget as a standalone window (no panel required)
run-widget:
    plasmawindowed {{widget_id}}

# Run the MCP service in stdio mode (dev)
run-stdio:
    cargo run -p tasks-mcp -- serve --stdio

# Run the MCP service in WebSocket mode (dev)
run-ws:
    cargo run -p tasks-mcp -- serve --ws

# Run the D-Bus standalone service (dev)
run-dbus:
    cargo run -p tasks-mcp -- dbus
