//! Thin orchestration over the PipeWire/WirePlumber CLI primitives.
//!
//! v1 drives `pw-loopback` (virtual source + routing), `pw-dump` (graph
//! introspection) and `wpctl` (default-source + mute). Swapping this module for
//! native libpipewire bindings later does not change the rest of the daemon.

use crate::config::{PUSHMUTE_DESCRIPTION, PUSHMUTE_NODE_NAME};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::process::{Child, Command, Stdio};

fn run(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("spawning `{cmd}`"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "`{cmd} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A capture source visible in the PipeWire graph.
pub struct CaptureDevice {
    pub id: u32,
    pub name: String,
    pub description: String,
}

/// Enumerate `Audio/Source` nodes, excluding PushMute's own virtual source.
pub fn list_capture_devices() -> Result<Vec<CaptureDevice>> {
    parse_capture_devices(&run("pw-dump", &[])?)
}

/// Resolve a node's object id by its `node.name`.
pub fn node_id_by_name(name: &str) -> Result<Option<u32>> {
    parse_node_id_by_name(&run("pw-dump", &[])?, name)
}

/// Confirm `pw-dump` runs and emits JSON of the expected shape. Used by `doctor`
/// and the `run` preflight; reuses the same pure parser as the live path so a
/// schema drift that would break routing is caught here first.
pub fn probe_pw_dump() -> Result<()> {
    parse_capture_devices(&run("pw-dump", &[])?).map(|_| ())
}

/// Confirm `wpctl` is alive and its output parseable. A *missing* default source
/// is benign (the daemon handles `None`), so `inspect` failing falls back to
/// `wpctl status` for a liveness check — only a wholly unresponsive `wpctl` fails.
pub fn probe_wpctl() -> Result<()> {
    match run("wpctl", &["inspect", "@DEFAULT_SOURCE@"]) {
        Ok(out) => {
            let _ = parse_default_source(&out);
            Ok(())
        }
        Err(_) => run("wpctl", &["status"])
            .map(|_| ())
            .context("wpctl is not responding"),
    }
}

/// The `node.name` of the current default source, if any.
pub fn current_default_source() -> Result<Option<String>> {
    let out = match run("wpctl", &["inspect", "@DEFAULT_SOURCE@"]) {
        Ok(o) => o,
        Err(_) => return Ok(None), // no default set
    };
    Ok(parse_default_source(&out))
}

// --- Pure parsers over the CLI text/JSON --------------------------------------
//
// The OS-facing surface is the *shape* of `pw-dump` JSON and `wpctl inspect` text,
// which has drifted across PipeWire/WirePlumber releases. Keeping the parsing pure
// (text in, values out) lets CI pin it against captured fixtures from old and new
// PipeWire without a live audio server — see the fixtures in `tests/fixtures/`.

/// Parse `pw-dump` JSON into capture devices, excluding PushMute's own source.
fn parse_capture_devices(dump: &str) -> Result<Vec<CaptureDevice>> {
    let v: Value = serde_json::from_str(dump).context("parsing pw-dump JSON")?;
    let mut out = Vec::new();
    for obj in v.as_array().into_iter().flatten() {
        if !is_node(obj) {
            continue;
        }
        let props = match obj.pointer("/info/props") {
            Some(p) => p,
            None => continue,
        };
        let class = props
            .get("media.class")
            .and_then(Value::as_str)
            .unwrap_or("");
        if class != "Audio/Source" {
            continue;
        }
        let name = props.get("node.name").and_then(Value::as_str).unwrap_or("");
        if name.is_empty() || name == PUSHMUTE_NODE_NAME {
            continue;
        }
        let description = props
            .get("node.description")
            .and_then(Value::as_str)
            .unwrap_or(name)
            .to_string();
        let id = obj.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
        out.push(CaptureDevice {
            id,
            name: name.to_string(),
            description,
        });
    }
    Ok(out)
}

/// Resolve a node's object id by its `node.name` from `pw-dump` JSON.
fn parse_node_id_by_name(dump: &str, name: &str) -> Result<Option<u32>> {
    let v: Value = serde_json::from_str(dump).context("parsing pw-dump JSON")?;
    for obj in v.as_array().into_iter().flatten() {
        if !is_node(obj) {
            continue;
        }
        let matches = obj.pointer("/info/props/node.name").and_then(Value::as_str) == Some(name);
        if matches {
            if let Some(id) = obj.get("id").and_then(Value::as_u64) {
                return Ok(Some(id as u32));
            }
        }
    }
    Ok(None)
}

/// The `node.name` of the default source from `wpctl inspect @DEFAULT_SOURCE@`
/// output. The line is marked with a leading `*` (a directly-set property).
fn parse_default_source(inspect: &str) -> Option<String> {
    for line in inspect.lines() {
        if let Some(rest) = line.trim().strip_prefix("* node.name = ") {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

/// A `pw-dump` object whose `type` names a Node (`PipeWire:Interface:Node`).
fn is_node(obj: &Value) -> bool {
    obj.get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .ends_with("Node")
}

/// Set the default source by `node.name`.
pub fn set_default_source(node_name: &str) -> Result<()> {
    let id = node_id_by_name(node_name)?
        .ok_or_else(|| anyhow!("source `{node_name}` not found in graph"))?;
    run("wpctl", &["set-default", &id.to_string()])?;
    Ok(())
}

/// Mute/unmute a node by object id. This is the hot path on every hotkey edge.
pub fn set_mute_id(id: u32, mute: bool) -> Result<()> {
    run(
        "wpctl",
        &["set-mute", &id.to_string(), if mute { "1" } else { "0" }],
    )?;
    Ok(())
}

/// Spawn `pw-loopback` to create the `pushmute` virtual source, fed by `physical`.
///
/// The capture side targets the physical mic (shared, no exclusive grab); the
/// playback side is exposed as an `Audio/Source` that all default-source clients
/// read from.
pub fn spawn_loopback(physical: &str) -> Result<Child> {
    let capture_props = format!("node.target={physical} node.passive=true");
    let playback_props = format!(
        "media.class=Audio/Source node.name={PUSHMUTE_NODE_NAME} node.description=\"{PUSHMUTE_DESCRIPTION}\""
    );
    let child = Command::new("pw-loopback")
        .args([
            "-m",
            "[ FL FR ]",
            "--capture-props",
            &capture_props,
            "--playback-props",
            &playback_props,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("spawning pw-loopback (is pipewire installed?)")?;
    Ok(child)
}

/// Poll the graph until `pushmute` appears, returning its node id.
pub fn wait_for_node(name: &str, attempts: u32) -> Result<u32> {
    for _ in 0..attempts {
        if let Some(id) = node_id_by_name(name)? {
            return Ok(id);
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    Err(anyhow!(
        "`{name}` did not appear in the graph after startup"
    ))
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    const PW_DUMP_OLD: &str = include_str!("../tests/fixtures/pw-dump-old.json");
    const PW_DUMP_NEW: &str = include_str!("../tests/fixtures/pw-dump-new.json");
    const WPCTL_DEFAULT: &str = include_str!("../tests/fixtures/wpctl-inspect-default-source.txt");

    /// Both fixtures describe the same two physical sources (a built-in and a USB
    /// mic), plus a sink, a non-node Device, and PushMute's own source — so the
    /// expected output is identical across the old and new PipeWire shapes.
    fn assert_two_sources(dump: &str) {
        let devices = parse_capture_devices(dump).expect("parse capture devices");
        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "alsa_input.pci-0000_00_1f.3.analog-stereo",
                "alsa_input.usb-Generic_USB_Mic-00.mono-fallback",
            ],
            "should list both physical sources, excluding the sink and pushmute",
        );
        // Descriptions fall through to node.description.
        assert_eq!(devices[1].description, "Generic USB Mic");
    }

    #[test]
    fn capture_devices_stable_across_pipewire_versions() {
        assert_two_sources(PW_DUMP_OLD);
        assert_two_sources(PW_DUMP_NEW);
    }

    #[test]
    fn capture_devices_excludes_pushmute_and_sinks() {
        for dump in [PW_DUMP_OLD, PW_DUMP_NEW] {
            let names: Vec<String> = parse_capture_devices(dump)
                .unwrap()
                .into_iter()
                .map(|d| d.name)
                .collect();
            assert!(!names.iter().any(|n| n == PUSHMUTE_NODE_NAME));
            assert!(!names.iter().any(|n| n.contains("alsa_output")));
        }
    }

    #[test]
    fn node_id_resolves_by_name() {
        // Ids differ between the fixtures; the lookup is by name, not position.
        assert_eq!(
            parse_node_id_by_name(PW_DUMP_OLD, "pushmute").unwrap(),
            Some(60)
        );
        assert_eq!(
            parse_node_id_by_name(PW_DUMP_NEW, "pushmute").unwrap(),
            Some(102)
        );
        assert_eq!(
            parse_node_id_by_name(PW_DUMP_NEW, "alsa_input.pci-0000_00_1f.3.analog-stereo")
                .unwrap(),
            Some(70)
        );
    }

    #[test]
    fn node_id_missing_is_none() {
        assert_eq!(
            parse_node_id_by_name(PW_DUMP_NEW, "does-not-exist").unwrap(),
            None
        );
    }

    #[test]
    fn default_source_parses_starred_node_name() {
        assert_eq!(
            parse_default_source(WPCTL_DEFAULT).as_deref(),
            Some("alsa_input.pci-0000_00_1f.3.analog-stereo")
        );
    }

    #[test]
    fn default_source_none_when_absent() {
        assert_eq!(
            parse_default_source("id 0, type Node\n    foo = \"bar\"\n"),
            None
        );
    }

    #[test]
    fn unparseable_dump_is_an_error() {
        assert!(parse_capture_devices("not json at all").is_err());
        assert!(parse_node_id_by_name("{ broken", "x").is_err());
    }
}
