# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.9] - 2026-06-28

### Added
- Show which link carries internet traffic: when WiFi and Ethernet are both up,
  the active default-route link is highlighted in green, with a box-footer
  caption naming it
- Switch the internet path with `u` — make the Ethernet row or connected WiFi
  the default route while keeping the other link up
- Filter the New Networks scan list by SSID with `/`
- Device box footer shows the active adapter's LAN IP (handy for SSH)

### Fixed
- Surface WiFi connect outcomes as notifications: report success, wrong
  password, SSID-not-found (out of range), and timeouts instead of failing
  silently
- Validate WPA/WPA2 passphrase length (8+ characters) before connecting,
  matching the existing hotspot check
- Esc now clears an applied SSID filter on the New Networks list
- Show Ethernet status when the WiFi radio is off, and the WiFi connected
  indicator when Ethernet is also active

## [0.1.8] - 2026-06-16

### Added
- VPN connections modal (press `v`): list saved VPN/WireGuard profiles and
  toggle them on/off
- Toggle a profile's autoconnect (`a`) and delete profiles (`d`) from the modal
- Always-on badge in the top-right showing active tunnels, plus assigned IP and
  uptime for the selected tunnel
- Import a WireGuard config with `i` — paste it directly or point at a `.conf`
  file (creates the NetworkManager profile, no `nmcli` needed)

## [0.1.4] - 2026-02-15

### Added
- Hidden network connection support (press `h` on New Networks)
- All-in-one dialog with SSID, security type, and password fields

### Fixed
- WPA3 now uses correct `sae` key management instead of `wpa-psk`

## [0.1.0] - 2024-12-24

### Added
- Initial release as `wlctl`
- NetworkManager D-Bus integration (replacing iwd)
- WPA Enterprise (802.1X) support via D-Bus API
- Station mode for WiFi client operations
- Access Point mode for hotspot functionality
- QR Code network sharing
- Known networks management
- Unit tests for core types

### Changed
- Migrated from iwd to NetworkManager backend
- Config location moved to `~/.config/wlctl/config.toml`
- 802.1X auth no longer requires root (uses D-Bus/PolicyKit)

### Credits
- Forked from [pythops/impala](https://github.com/pythops/impala)
