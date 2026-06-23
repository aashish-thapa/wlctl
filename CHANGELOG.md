# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Show which link carries internet traffic: when WiFi and Ethernet are both up,
  the active default-route link is highlighted in green
- Switch the internet path with `u` — make the Ethernet row or connected WiFi
  the default route while keeping the other link up

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
