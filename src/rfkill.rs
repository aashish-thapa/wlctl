use anyhow::Result;
use std::fs;

pub fn check() -> Result<()> {
    let entries = fs::read_dir("/sys/class/rfkill/")?;

    for entry in entries {
        let entry = entry?;
        let entry_path = entry.path();

        if let Some(_file_name) = entry_path.file_name() {
            let name = fs::read_to_string(entry_path.join("type"))?;

            if name.trim() == "wlan" {
                let state_path = entry_path.join("state");
                let state = fs::read_to_string(state_path)?.trim().parse::<u8>()?;

                // https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-class-rfkill
                //
                // Only a hard block (state 2, physical/BIOS kill switch) is
                // fatal: software can't clear it. A soft block (state 0) is
                // usually NetworkManager's own block from a prior power-off, so
                // let the app launch — it shows the device as off, and toggling
                // power re-enables wireless over D-Bus (no sudo, no rfkill).
                if state == 2 {
                    eprintln!(
                        "The wifi device is hard blocked. Enable it with the hardware switch or in BIOS."
                    );
                    std::process::exit(1);
                }
                break;
            }
        }
    }
    Ok(())
}
