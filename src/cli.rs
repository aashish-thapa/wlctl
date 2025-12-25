use clap::{Command, arg, crate_version};

pub fn cli() -> Command {
    Command::new("wlctl")
        .about("TUI for managing WiFi using NetworkManager")
        .version(crate_version!())
        .arg(
            arg!(--mode <mode>)
                .short('m')
                .required(false)
                .help("Device mode")
                .value_parser(["station", "ap"]),
        )
}
