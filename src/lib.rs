pub mod app;

pub mod event;

pub mod ui;

pub mod tui;

pub mod handler;

pub mod config;

pub mod notification;

pub mod device;

pub mod adapter;

pub mod cli;

pub mod rfkill;

pub mod mode;

pub mod reset;

pub mod agent;

pub mod nm;

pub fn nm_network_name(name: &str) -> String {
    // NetworkManager handles SSID encoding internally, so we just return as-is
    name.to_string()
}
