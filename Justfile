set shell := ["bash", "-c"]

PREFIX := "/usr/local"

default:
    @just --list

build:
    cargo build

build-release:
    cargo build --release

run:
    cargo run

lint:
    cargo clippy -- -D warnings
    cargo fmt --check

test:
    cargo test

install-local: build-release
    mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/dbus-1/services ~/.local/share/xdg-desktop-portal/portals ~/.config/xdg-desktop-portal
    install -Dm755 target/release/stellarfiles ~/.local/bin/stellarfiles
    install -Dm644 org.freedesktop.impl.portal.desktop.stellarfiles.desktop ~/.local/share/applications/org.freedesktop.impl.portal.desktop.stellarfiles.desktop
    install -Dm644 stellarfiles.portal ~/.local/share/xdg-desktop-portal/portals/stellarfiles.portal
    
    printf "[D-BUS Service]\nName=org.freedesktop.impl.portal.desktop.stellarfiles\nExec=%s/.local/bin/stellarfiles\n" "$HOME" > ~/.local/share/dbus-1/services/org.freedesktop.impl.portal.desktop.stellarfiles.service
    
    xdg-mime default org.freedesktop.impl.portal.desktop.stellarfiles.desktop inode/directory
    update-desktop-database ~/.local/share/applications || true
    
    touch ~/.config/xdg-desktop-portal/portals.conf
    sed -i '/org.freedesktop.impl.portal.FileChooser/d' ~/.config/xdg-desktop-portal/portals.conf
    grep -q "\[preferred\]" ~/.config/xdg-desktop-portal/portals.conf || echo "[preferred]" >> ~/.config/xdg-desktop-portal/portals.conf
    sed -i '/\[preferred\]/a org.freedesktop.impl.portal.FileChooser=stellarfiles' ~/.config/xdg-desktop-portal/portals.conf
    cp ~/.config/xdg-desktop-portal/portals.conf ~/.config/xdg-desktop-portal/cosmic-portals.conf
    
    systemctl --user daemon-reload
    systemctl --user reload dbus.service || killall -HUP dbus-daemon || true
    systemctl --user restart xdg-desktop-portal
    
    @echo "Checking if ~/.local/bin is in PATH..."
    @if ! echo $PATH | grep -q "$HOME/.local/bin"; then \
        echo "Adding ~/.local/bin to PATH..."; \
        grep -q 'export PATH="$HOME/.local/bin:$PATH"' ~/.bashrc || echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc; \
        if [ -f ~/.zshrc ]; then grep -q 'export PATH="$HOME/.local/bin:$PATH"' ~/.zshrc || echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc; fi; \
        echo "\n[!] Please restart your terminal or run 'source ~/.bashrc' for the 'stellarfiles' command to work."; \
    fi
    @echo "Local installation complete."

uninstall-local:
    rm -f ~/.local/bin/stellarfiles
    rm -f ~/.local/share/applications/org.freedesktop.impl.portal.desktop.stellarfiles.desktop
    rm -f ~/.local/share/xdg-desktop-portal/portals/stellarfiles.portal
    rm -f ~/.local/share/dbus-1/services/org.freedesktop.impl.portal.desktop.stellarfiles.service
    sed -i '/org.freedesktop.impl.portal.FileChooser=stellarfiles/d' ~/.config/xdg-desktop-portal/portals.conf || true
    sed -i '/org.freedesktop.impl.portal.FileChooser=stellarfiles/d' ~/.config/xdg-desktop-portal/cosmic-portals.conf || true
    update-desktop-database ~/.local/share/applications || true
    systemctl --user restart xdg-desktop-portal || true
    @echo "Local uninstallation complete."

install-system: build-release
    @if [ "$EUID" -ne 0 ]; then echo "Please run as root (sudo just install-system)"; exit 1; fi
    install -Dm755 target/release/stellarfiles {{PREFIX}}/bin/stellarfiles
    install -Dm644 org.freedesktop.impl.portal.desktop.stellarfiles.desktop /usr/share/applications/org.freedesktop.impl.portal.desktop.stellarfiles.desktop
    install -Dm644 org.freedesktop.impl.portal.desktop.stellarfiles.service /usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.stellarfiles.service
    install -Dm644 stellarfiles.portal /usr/share/xdg-desktop-portal/portals/stellarfiles.portal
    
    update-desktop-database /usr/share/applications || true
    @echo "System installation complete."
    @echo "Note: Run 'xdg-mime default org.freedesktop.impl.portal.desktop.stellarfiles.desktop inode/directory' and 'systemctl --user restart xdg-desktop-portal' without sudo to apply to your user."

uninstall-system:
    @if[ "$EUID" -ne 0 ]; then echo "Please run as root (sudo just uninstall-system)"; exit 1; fi
    rm -f {{PREFIX}}/bin/stellarfiles
    rm -f /usr/share/applications/org.freedesktop.impl.portal.desktop.stellarfiles.desktop
    rm -f /usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.stellarfiles.service
    rm -f /usr/share/xdg-desktop-portal/portals/stellarfiles.portal
    update-desktop-database /usr/share/applications || true
    @echo "System uninstallation complete."

clean:
    cargo clean