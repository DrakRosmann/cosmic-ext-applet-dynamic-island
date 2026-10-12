// SPDX-License-Identifier: GPL-3.0-only

//! Apps recording from a microphone, as phones show them: whoever records,
//! the Sound Recorder or a call, from PulseAudio's view of PipeWire.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

use cosmic::iced::{Subscription, stream};
use futures::{SinkExt, channel::mpsc};
use serde_json::Value;
use tokio::io::AsyncBufReadExt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Recorder {
    /// Who records: the program, or the app's name when it gives no program.
    pub key: String,
    /// As its launcher names it, like "Sound Recorder".
    pub name: String,
    /// Its icon's file, found once.
    pub icon: Option<PathBuf>,
}

async fn list(what: &str) -> Vec<Value> {
    let Ok(output) = tokio::process::Command::new("pactl")
        .args(["-f", "json", "list", what])
        .output()
        .await
    else {
        return Vec::new();
    };
    serde_json::from_slice(&output.stdout).unwrap_or_default()
}

fn property<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    object["properties"][key].as_str().filter(|v| !v.is_empty())
}

/// The program behind a stream: named, or in the brackets ALSA's plugin
/// puts it in, "PipeWire ALSA [cosmic-ext-sound-recorder]", as cpal records.
fn program(output: &Value) -> Option<String> {
    if let Some(binary) = property(output, "application.process.binary") {
        return Some(binary.to_owned());
    }
    let name = property(output, "application.name")?;
    let inner = name.strip_prefix("PipeWire ALSA [")?.strip_suffix(']')?;
    Some(inner.to_owned())
}

/// Apps recording from a microphone: not from a monitor of an output, which
/// is sound capture, nor level meters, nor streams on hold.
async fn recorders() -> Vec<Recorder> {
    let monitors: HashSet<u64> = list("sources")
        .await
        .iter()
        .filter(|source| {
            property(source, "device.class") == Some("monitor")
                || source["name"].as_str().is_some_and(|n| n.ends_with(".monitor"))
        })
        .filter_map(|source| source["index"].as_u64())
        .collect();
    let mut recorders: Vec<Recorder> = Vec::new();
    for output in list("source-outputs").await {
        let from_monitor = output["source"].as_u64().is_some_and(|s| monitors.contains(&s));
        let meter = property(&output, "stream.monitor") == Some("true")
            || property(&output, "media.name") == Some("Peak detect");
        if from_monitor || meter || output["corked"].as_bool().unwrap_or(false) {
            continue;
        }
        let app_id = property(&output, "pipewire.access.portal.app_id")
            .or_else(|| property(&output, "application.id"));
        let program = program(&output);
        let Some(key) = program
            .clone()
            .or_else(|| app_id.map(str::to_owned))
            .or_else(|| property(&output, "application.name").map(str::to_owned))
        else {
            continue;
        };
        if recorders.iter().any(|r| r.key == key) {
            continue;
        }
        let entry = launcher(app_id, program.as_deref());
        // Without a launcher, the program's name over ALSA's "PipeWire ALSA […]".
        let name = entry
            .as_ref()
            .map(|e| e.name.clone())
            .or_else(|| program.clone().filter(|_| property(&output, "application.process.binary").is_none()))
            .or_else(|| property(&output, "application.name").map(str::to_owned))
            .unwrap_or_else(|| key.clone());
        let icon = entry
            .and_then(|e| e.icon)
            .or_else(|| property(&output, "application.icon_name").map(str::to_owned));
        recorders.push(Recorder {
            key,
            name,
            icon: icon.and_then(|icon| icon_path(&icon)),
        });
    }
    recorders
}

#[derive(Clone, Debug)]
struct Launcher {
    name: String,
    icon: Option<String>,
}

fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Where the user's data is: in a Flatpak sandbox `XDG_DATA_HOME` is the
/// app's own, the user's is told apart.
fn data_home() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let key = if in_flatpak() { "HOST_XDG_DATA_HOME" } else { "XDG_DATA_HOME" };
    std::env::var_os(key)
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
}

/// Where Flatpak puts its apps' launchers and icons, for the user and for all.
fn flatpak_exports() -> [PathBuf; 2] {
    [
        data_home().join("flatpak/exports/share"),
        PathBuf::from("/var/lib/flatpak/exports/share"),
    ]
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![data_home().join("applications")];
    // The sandbox's own are the runtime's: the system's aren't seen.
    if !in_flatpak() {
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
        dirs.extend(
            data_dirs
                .split(':')
                .filter(|d| !d.is_empty())
                .map(|d| PathBuf::from(d).join("applications")),
        );
    }
    dirs.extend(flatpak_exports().map(|share| share.join("applications")));
    dirs
}

/// An app's icon: from the icon theme, or as Flatpak exports its apps'.
fn icon_path(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        return Some(PathBuf::from(name)).filter(|p| p.exists());
    }
    cosmic::widget::icon::from_name(name).size(32).path().or_else(|| {
        flatpak_exports().iter().find_map(|share| {
            let hicolor = share.join("icons/hicolor");
            std::iter::once(hicolor.join(format!("scalable/apps/{name}.svg")))
                .chain(
                    ["256x256", "128x128", "64x64", "48x48", "32x32"]
                        .iter()
                        .map(|size| hicolor.join(format!("{size}/apps/{name}.png"))),
                )
                .find(|path| path.exists())
        })
    })
}

/// A launcher's name, in the session's language if it has one, and icon.
fn parse(text: &str) -> Option<(Launcher, String)> {
    let language = std::env::var("LANG").unwrap_or_default();
    let language = language.split('.').next().unwrap_or_default();
    let short = language.split('_').next().unwrap_or_default();
    let (mut name, mut localized, mut icon, mut exec) = (None, None, None, String::new());
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.to_owned()),
            k if !language.is_empty()
                && (k == format!("Name[{language}]") || k == format!("Name[{short}]")) =>
            {
                localized = Some(value.to_owned());
            }
            "Icon" => icon = Some(value.to_owned()),
            "Exec" => exec = value.to_owned(),
            _ => {}
        }
    }
    Some((
        Launcher {
            name: localized.or(name)?,
            icon,
        },
        exec,
    ))
}

/// Whether a launcher's command runs this program: directly, or in its
/// Flatpak, `flatpak run --command=program`.
fn runs(exec: &str, program: &str) -> bool {
    exec.split_whitespace().any(|word| {
        let word = word.trim_start_matches("--command=");
        word.rsplit('/').next() == Some(program)
    })
}

/// The launcher of an app, by its ID or the program it runs. Looked for once.
fn launcher(app_id: Option<&str>, program: Option<&str>) -> Option<Launcher> {
    static FOUND: Mutex<Option<HashMap<String, Option<Launcher>>>> = Mutex::new(None);
    let key = format!("{app_id:?}/{program:?}");
    let mut found = FOUND.lock().ok()?;
    let found = found.get_or_insert_with(HashMap::new);
    if let Some(launcher) = found.get(&key) {
        return launcher.clone();
    }
    let dirs = application_dirs();
    let by_id = app_id.and_then(|id| {
        dirs.iter().find_map(|dir| {
            let text = std::fs::read_to_string(dir.join(format!("{id}.desktop"))).ok()?;
            parse(&text).map(|(launcher, _)| launcher)
        })
    });
    let launcher = by_id.or_else(|| {
        let program = program?;
        dirs.iter().find_map(|dir| {
            std::fs::read_dir(dir).ok()?.flatten().find_map(|file| {
                let path = file.path();
                if path.extension()? != "desktop" {
                    return None;
                }
                let (launcher, exec) = parse(&std::fs::read_to_string(&path).ok()?)?;
                runs(&exec, program).then_some(launcher)
            })
        })
    });
    found.insert(key, launcher.clone());
    launcher
}

pub fn subscription() -> Subscription<Vec<Recorder>> {
    Subscription::run_with("recording", |_| {
        stream::channel(4, |mut output: mpsc::Sender<Vec<Recorder>>| async move {
            let mut last = None;
            loop {
                // As streams come and go; read again if pactl ends.
                let child = tokio::process::Command::new("pactl")
                    .arg("subscribe")
                    .stdout(std::process::Stdio::piped())
                    .kill_on_drop(true)
                    .spawn();
                let mut lines = child
                    .ok()
                    .and_then(|mut child| {
                        let stdout = child.stdout.take()?;
                        Some((child, tokio::io::BufReader::new(stdout).lines()))
                    });
                loop {
                    let recorders = recorders().await;
                    if last.as_ref() != Some(&recorders) {
                        last = Some(recorders.clone());
                        if output.send(recorders).await.is_err() {
                            return;
                        }
                    }
                    match &mut lines {
                        Some((_, lines)) => loop {
                            match lines.next_line().await {
                                Ok(Some(line)) if line.contains("source-output") => break,
                                Ok(Some(_)) => continue,
                                _ => {
                                    tokio::time::sleep(Duration::from_secs(5)).await;
                                    break;
                                }
                            }
                        },
                        None => tokio::time::sleep(Duration::from_secs(5)).await,
                    }
                    if lines.as_mut().is_some_and(|(child, _)| child.try_wait().ok().flatten().is_some()) {
                        break;
                    }
                    // A burst of changes, read once.
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        })
    })
}
