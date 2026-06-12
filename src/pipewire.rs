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
    let dump = run("pw-dump", &[])?;
    let v: Value = serde_json::from_str(&dump)?;
    let mut out = Vec::new();
    for obj in v.as_array().into_iter().flatten() {
        if !obj.get("type").and_then(Value::as_str).unwrap_or("").ends_with("Node") {
            continue;
        }
        let props = match obj.pointer("/info/props") {
            Some(p) => p,
            None => continue,
        };
        let class = props.get("media.class").and_then(Value::as_str).unwrap_or("");
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
        out.push(CaptureDevice { id, name: name.to_string(), description });
    }
    Ok(out)
}

/// Resolve a node's object id by its `node.name`.
pub fn node_id_by_name(name: &str) -> Result<Option<u32>> {
    let dump = run("pw-dump", &[])?;
    let v: Value = serde_json::from_str(&dump)?;
    for obj in v.as_array().into_iter().flatten() {
        if !obj.get("type").and_then(Value::as_str).unwrap_or("").ends_with("Node") {
            continue;
        }
        let matches = obj
            .pointer("/info/props/node.name")
            .and_then(Value::as_str)
            == Some(name);
        if matches {
            if let Some(id) = obj.get("id").and_then(Value::as_u64) {
                return Ok(Some(id as u32));
            }
        }
    }
    Ok(None)
}

/// The `node.name` of the current default source, if any.
pub fn current_default_source() -> Result<Option<String>> {
    let out = match run("wpctl", &["inspect", "@DEFAULT_SOURCE@"]) {
        Ok(o) => o,
        Err(_) => return Ok(None), // no default set
    };
    for line in out.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("* node.name = ") {
            return Ok(Some(rest.trim_matches('"').to_string()));
        }
    }
    Ok(None)
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
    run("wpctl", &["set-mute", &id.to_string(), if mute { "1" } else { "0" }])?;
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
    Err(anyhow!("`{name}` did not appear in the graph after startup"))
}
