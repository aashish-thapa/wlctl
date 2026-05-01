<div align="center">
  <h1>wlctl</h1>
  <p>WiFi TUI for NetworkManager.</p>

  [![CI](https://github.com/aashish-thapa/wlctl/actions/workflows/ci.yaml/badge.svg)](https://github.com/aashish-thapa/wlctl/actions/workflows/ci.yaml)
  [![Crates.io](https://img.shields.io/crates/v/wlctl.svg)](https://crates.io/crates/wlctl)
  [![Downloads](https://img.shields.io/crates/d/wlctl.svg)](https://crates.io/crates/wlctl)
  [![License](https://img.shields.io/crates/l/wlctl.svg)](https://github.com/aashish-thapa/wlctl/blob/main/LICENSE)
</div>

![demo](https://github.com/user-attachments/assets/55c800ff-d0aa-4454-aa6b-3990833ce530)

## Why

[impala](https://github.com/pythops/impala) is a great wifi TUI but it talks to `iwd`. If your distro already runs NetworkManager (most do — GNOME, KDE, Ubuntu, Fedora, Pop, …), you can't drop impala in without ripping the stack out. wlctl keeps the impala UX and points it at NetworkManager, so it just works alongside what your DE is already doing.

## Features

- Station and Access Point modes
- WPA Enterprise (802.1X)
- Multiple adapters — pick which one to drive, switch on the fly
- `wlctl doctor` — walks rfkill, driver, association, IP, DHCP, gateway, DNS, internet
- QR code sharing, hidden networks, speed test
- Vim keys, every binding configurable

## Install

```sh
# crates.io
cargo install wlctl

# Arch (AUR)
yay -S wlctl-bin

# from source
git clone https://github.com/aashish-thapa/wlctl && cd wlctl
cargo build --release
```

Needs NetworkManager running. [Nerd Fonts](https://www.nerdfonts.com/) optional, for icons.

## Usage

`wlctl` to launch the TUI. `wlctl doctor` when something's broken and you want to know which layer.

| Action | Key |
|---|---|
| Switch panel | `Tab` / `Shift+Tab` |
| Move | `j` `k` / arrows |
| Scan | `s` |
| Connect / disconnect | `Space` or `Enter` |
| Hidden network | `h` |
| Toggle auto-connect | `t` |
| Forget network | `d` |
| Show all known | `a` |
| QR share | `p` |
| Speed test (needs `speedtest-cli`) | `Shift+S` |
| Device info | `i` |
| Toggle power | `o` |
| Switch adapter mode | `Ctrl+R` |
| Doctor | `?` |
| Quit | `q` / `Ctrl+C` |
| Dismiss popup | `Esc` |

## Config

`~/.config/wlctl/config.toml`. All keys rebindable.

```toml
switch = "r"
mode = "station"
esc_quit = false

[device]
infos = "i"
toggle_power = "o"
doctor = "?"

[station]
toggle_scanning = "s"

[station.known_network]
toggle_autoconnect = "t"
remove = "d"
show_all = "a"
share = "p"
speed_test = "S"

[station.new_network]
show_all = "a"
connect_hidden = "h"

[access_point]
start = "n"
stop = "x"
```

## vs. impala

|  | impala | wlctl |
|---|---|---|
| Backend | iwd | NetworkManager |
| Coexists with default desktop network stack | no | yes |
| Multi-adapter selector | — | yes |
| `doctor` subcommand | — | yes |

## Credits

Forked from [pythops/impala](https://github.com/pythops/impala). UI and architecture are theirs.

## License

GPLv3
