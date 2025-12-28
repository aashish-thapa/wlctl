<div align="center">
  <h1>wlctl</h1>
  <h3>TUI for managing WiFi using NetworkManager</h3>
  <p>A fork of <a href="https://github.com/pythops/impala">impala</a> that uses NetworkManager instead of iwd</p>

  [![CI](https://github.com/aashish-thapa/wlctl/actions/workflows/ci.yaml/badge.svg)](https://github.com/aashish-thapa/wlctl/actions/workflows/ci.yaml)
  [![Crates.io](https://img.shields.io/crates/v/wlctl.svg)](https://crates.io/crates/wlctl)
  [![Downloads](https://img.shields.io/crates/d/wlctl.svg)](https://crates.io/crates/impala-nm)
  [![License](https://img.shields.io/crates/l/wlctl.svg)](https://github.com/aashish-thapa/wlctl/blob/main/LICENSE)
</div>

## Purpose

I personally love impala but it has limitation due to use of iwd. So, this repo preserves the beauty but changes the underlying logic to use NetworkManager.

## 📸 Demo

![](https://github.com/user-attachments/assets/55c800ff-d0aa-4454-aa6b-3990833ce530)

## ✨ Features

- WPA Enterprise (802.1X) Support
- Station & Access Point Modes
- QR Code Network Sharing
- **Uses NetworkManager** - works alongside your existing network setup

## 💡 Prerequisites

- A Linux based OS
- [NetworkManager](https://networkmanager.dev/) running
- [nerdfonts](https://www.nerdfonts.com/) (Optional) for icons

> [!NOTE]
> This fork uses NetworkManager instead of iwd, so it works with your existing network configuration without conflicts.

## 🚀 Installation

### 📦 crates.io

You can install `wlctl` from [crates.io](https://crates.io/crates/wlctl)

```shell
cargo install wlctl
```
## Arch Linux (AUR)

`wlctl-bin` is available on the Arch User Repository.

```bash
yay -S wlctl-bin
```

### ⚒️ Build from source

Run the following command:

```shell
git clone https://github.com/aashish-thapa/wlctl
cd wlctl
cargo build --release
```

This will produce an executable file at `target/release/wlctl` that you can copy to a directory in your `$PATH`.

## 🪄 Usage

### Global

`Tab` or `Shift + Tab`: Switch between different sections.

`j` or `Down` : Scroll down.

`k` or `Up`: Scroll up.

`ctrl+r`: Switch adapter mode.

`?`: Show help.

`esc`: Dismiss the different pop-ups.

`q` or `ctrl+c`: Quit the app. (Note: `<Esc>` can also quit if `esc_quit = true` is set in config)

### Device

`i`: Show device information.

`o`: Toggle device power.

### Station

`s`: Start scanning.

`Space or Enter`: Connect/Disconnect the network.

### Known Networks

`t`: Enable/Disable auto-connect.

`d`: Remove the network from the known networks list.

`a`: Show all the known networks.

`p`: Share via QR Code.

`Shift + s`: Speed test 
> [!NOTE]
> `speedtest-cli` is used. Install it with `pip install speedtest-cli`

### Access Point

`n`: Start a new access point.

`x`: Stop the running access point.

## Custom keybindings

Keybindings can be customized in the config file `$HOME/.config/wlctl/config.toml`

```toml
switch = "r"
mode = "station"
esc_quit = false  # Set to true to enable Esc key to quit the app

[device]
infos = "i"
toggle_power = "o"

[access_point]
start = 'n'
stop = 'x'

[station]
toggle_scanning = "s"

[station.known_network]
toggle_autoconnect = "t"
remove = "d"
show_all = "a"
share = "p"
```

## Differences from upstream impala

| Feature | impala (upstream) | wlctl (this fork) |
|---------|-------------------|-------------------|
| Backend | iwd | NetworkManager |
| Config location | `/var/lib/iwd/` | `/etc/NetworkManager/system-connections/` |
| Conflicts | Conflicts with NetworkManager | Works alongside existing setup |

## Credits

This is a fork of [pythops/impala](https://github.com/pythops/impala). All credit for the original UI and architecture goes to the original author.

## ⚖️ License

GPLv3
