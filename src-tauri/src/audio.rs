use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const READ_CHUNK: usize = 4096;

pub struct AudioCapture {
    pub child: Child,
}

/// Spawn the Swift `audio-capture` sidecar.  Returns a channel of raw 16 kHz
/// mono Int16 LE PCM bytes (Bytes per chunk) and a handle to the child process.
///
/// `cancel` is used to terminate the sidecar when the meeting stops.
pub async fn start_capture(
    app: &AppHandle,
    cancel: CancellationToken,
    include_mic: bool,
) -> Result<mpsc::Receiver<Bytes>> {
    use tauri::Emitter;
    let app_for_meta = app.clone();
    let bin = resolve_sidecar(app)?;
    tracing::info!(?bin, "spawning audio sidecar");

    let mut cmd = Command::new(&bin);
    if !include_mic {
        cmd.arg("--no-mic");
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().with_context(|| format!("spawn {:?}", bin))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("no stdout from sidecar"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("no stderr from sidecar"))?;
    // Keep stdin so we can drop it on cancel — closing the pipe lets the
    // sidecar exit cleanly via its EOF watcher before we resort to SIGKILL.
    let stdin = child.stdin.take();

    let (tx, rx) = mpsc::channel::<Bytes>(64);

    // Pipe stderr -> tracing. META:level lines are converted to an
    // audio:level event so the top-bar VU meter can render.
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(rest) = line.strip_prefix("META:level ") {
                let mut mic: f32 = 0.0;
                let mut sys: f32 = 0.0;
                for kv in rest.split_whitespace() {
                    if let Some(v) = kv.strip_prefix("mic=") {
                        mic = v.parse().unwrap_or(0.0);
                    } else if let Some(v) = kv.strip_prefix("sys=") {
                        sys = v.parse().unwrap_or(0.0);
                    }
                }
                let _ = app_for_meta.emit(
                    "audio:level",
                    serde_json::json!({ "mic": mic, "sys": sys }),
                );
            } else if let Some(rest) = line.strip_prefix("ERR ") {
                tracing::warn!(target: "audio_sidecar", "{}", rest);
            } else if let Some(rest) = line.strip_prefix("LOG ") {
                tracing::info!(target: "audio_sidecar", "{}", rest);
            } else {
                tracing::info!(target: "audio_sidecar", "{}", line);
            }
        }
    });

    // Pipe stdout (raw PCM) -> mpsc channel
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; READ_CHUNK];
            let mut stdout = stdout;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    res = stdout.read(&mut buf) => match res {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            if tx.send(Bytes::copy_from_slice(&buf[..n])).await.is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            tracing::warn!(?err, "audio stdout read error");
                            break;
                        }
                    }
                }
            }
            tracing::info!("audio stdout pipe closed");
        });
    }

    // Cancel-on-shutdown: close stdin & kill child if cancellation arrives.
    tokio::spawn(async move {
        cancel.cancelled().await;
        // Drop stdin to signal sidecar to exit, then ensure it dies.
        drop(stdin);
        // Give it a moment to exit gracefully, then kill.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let _ = child.start_kill();
        let _ = child.wait().await;
    });

    Ok(rx)
}

fn resolve_sidecar(app: &AppHandle) -> Result<PathBuf> {
    let triple = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else {
        return Err(anyhow!("unsupported architecture for audio sidecar"));
    };
    let bundled_name = format!("audio-capture-{triple}");

    // 1. Sibling of the executable. In Tauri 2 production builds, externalBin
    //    binaries are placed in `Contents/MacOS/` next to the main binary —
    //    NOT in `Contents/Resources/`. Tauri strips the target-triple suffix
    //    when bundling, so the file is named `audio-capture` rather than
    //    `audio-capture-aarch64-apple-darwin` in the installed app.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [bundled_name.as_str(), "audio-capture"] {
                let p = dir.join(name);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }
    // 2. Bundled resource location (some Tauri configurations route here).
    if let Ok(p) = app.path().resolve(&bundled_name, BaseDirectory::Resource) {
        if p.exists() {
            return Ok(p);
        }
    }
    // 3. Dev location: <crate>/binaries/audio-capture-<triple>
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(&bundled_name);
    if dev.exists() {
        return Ok(dev);
    }
    Err(anyhow!(
        "audio sidecar not found. Expected `{}` next to the app binary, in \
         the resource dir, or under `src-tauri/binaries/`. Run \
         `npm run build:swift` to compile it.",
        bundled_name
    ))
}
