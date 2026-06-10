//! Selective Mic Router — talk to one app without being heard by the rest.

mod config;
mod daemon;
mod input;
mod ipc;
mod pipewire;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;

#[derive(Parser)]
#[command(name = "smr", version, about = "Selective Mic Router")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the router (default if no subcommand is given).
    Run,
    /// Print the running daemon's status.
    Status,
    /// List capture devices and input devices.
    Devices,
    /// Set the physical mic to route from (by node.name).
    SetMic { name: String },
    /// Bind the push-to-talk key by capturing the next press.
    SetKey {
        /// Optional specific /dev/input/eventN to capture from.
        #[arg(long)]
        device: Option<String>,
    },
    /// Mute the virtual source (sends to the running daemon).
    Mute {
        /// Accepted for symmetry with the PRD CLI; mute is held until `unmute`.
        #[arg(long)]
        hold: bool,
    },
    /// Unmute the virtual source.
    Unmute,
    /// Toggle mute.
    Toggle,
    /// Restore the default source recorded before SMR changed it.
    Restore,
    /// Reload config (currently: validate; live mic change needs a restart).
    Reload,
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("smr: error: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Run) {
        Command::Run => daemon::run(Config::load()?),
        Command::Status => {
            println!("{}", ipc::send("status")?);
            Ok(())
        }
        Command::Devices => devices(),
        Command::SetMic { name } => set_mic(name),
        Command::SetKey { device } => set_key(device),
        Command::Mute { .. } => {
            println!("{}", ipc::send("mute")?);
            Ok(())
        }
        Command::Unmute => {
            println!("{}", ipc::send("unmute")?);
            Ok(())
        }
        Command::Toggle => {
            println!("{}", ipc::send("toggle")?);
            Ok(())
        }
        Command::Restore => daemon::restore(&Config::load()?),
        Command::Reload => {
            let cfg = Config::load()?;
            println!(
                "config OK: mic={:?} ptt_keycode={:?} set_default={}",
                cfg.physical_mic, cfg.ptt_keycode, cfg.set_default
            );
            println!("(note: changing the routed mic requires restarting the daemon)");
            Ok(())
        }
    }
}

fn devices() -> Result<()> {
    let cfg = Config::load()?;
    println!("Capture devices (use the node.name with `smr set-mic`):");
    for d in pipewire::list_capture_devices()? {
        let marker = if cfg.physical_mic.as_deref() == Some(&d.name) {
            " *"
        } else {
            "  "
        };
        println!("{marker} [{:>3}] {}", d.id, d.description);
        println!("        {}", d.name);
    }
    println!("\nInput devices (for `smr set-key --device`):");
    for (path, dev) in evdev::enumerate() {
        if dev.supported_events().contains(evdev::EventType::KEY) {
            println!("   {}  {}", path.display(), dev.name().unwrap_or("?"));
        }
    }
    Ok(())
}

fn set_mic(name: String) -> Result<()> {
    let mut cfg = Config::load()?;
    let known: Vec<String> = pipewire::list_capture_devices()?
        .into_iter()
        .map(|d| d.name)
        .collect();
    if !known.contains(&name) {
        eprintln!("warning: `{name}` is not among the current capture devices:");
        for n in &known {
            eprintln!("  {n}");
        }
    }
    cfg.physical_mic = Some(name.clone());
    cfg.save()?;
    println!("physical mic set → {name}");
    Ok(())
}

fn set_key(device: Option<String>) -> Result<()> {
    println!("Press the key you want to use for push-to-talk…");
    let code = input::capture_keycode(device.clone())?;
    let mut cfg = Config::load()?;
    cfg.ptt_keycode = Some(code);
    cfg.ptt_device = device;
    cfg.save()?;
    println!("push-to-talk bound → evdev keycode {code}");
    Ok(())
}
