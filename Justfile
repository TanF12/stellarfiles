default:
    @just --list

build:
    cargo build

build-release:
    cargo build --release

run:
    cargo run

install-local: build-release
    mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/dbus-1/services ~/.local/share/xdg-desktop-portal/portals ~/.config/xdg-desktop-portal
    cp target/release/stellarfiles ~/.local/bin/
    cp stellarfiles.desktop ~/.local/share/applications/
    cp stellarfiles.portal ~/.local/share/xdg-desktop-portal/portals/
    
    printf "[D-BUS Service]\nName=org.freedesktop.impl.portal.desktop.stellarfiles\nExec={{env_var("HOME")}}/.local/bin/stellarfiles\n" > ~/.local/share/dbus-1/services/org.freedesktop.impl.portal.desktop.stellarfiles.service
    
    xdg-mime default stellarfiles.desktop inode/directory
    
    touch ~/.config/xdg-desktop-portal/portals.conf
    sed -i '/org.freedesktop.impl.portal.FileChooser/d' ~/.config/xdg-desktop-portal/portals.conf
    grep -q "\[preferred\]" ~/.config/xdg-desktop-portal/portals.conf || echo "[preferred]" >> ~/.config/xdg-desktop-portal/portals.conf
    sed -i '/\[preferred\]/a org.freedesktop.impl.portal.FileChooser=stellarfiles' ~/.config/xdg-desktop-portal/portals.conf
    cp ~/.config/xdg-desktop-portal/portals.conf ~/.config/xdg-desktop-portal/cosmic-portals.conf
    
    systemctl --user daemon-reload
    systemctl --user reload dbus.service || killall -HUP dbus-daemon || true
    systemctl --user restart xdg-desktop-portal
    
    @echo "Local installation complete. Stellarfiles is now natively bound to DBus and Cosmic."

install-system: build-release
    cp target/release/stellarfiles /usr/local/bin/
    cp stellarfiles.desktop /usr/share/applications/
    cp org.freedesktop.impl.portal.desktop.stellarfiles.service /usr/share/dbus-1/services/
    cp stellarfiles.portal /usr/share/xdg-desktop-portal/portals/
    xdg-mime default stellarfiles.desktop inode/directory
    systemctl --user restart xdg-desktop-portal
    @echo "System installation complete."

test:
    cargo test

clean:
    cargo clean