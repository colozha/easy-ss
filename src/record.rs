use crate::capture::{storage_dir, unique_path};
use crate::settings::{Language, Storage};
use ashpd::WindowIdentifier;
use ashpd::desktop::{
    PersistMode, ResponseError, Session,
    screencast::{CursorMode, Screencast, SourceType},
};
use gtk::{gio, glib, prelude::*};
use std::fs;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::time::Duration;

const SIGINT: i32 = 2;

/// A running screen recording: a portal screencast session encoded to WebM
/// by `gst-launch-1.0`.
pub struct Recording {
    pub path: PathBuf,
    pub process: gio::Subprocess,
    session: Session<'static, Screencast<'static>>,
    _portal: Screencast<'static>,
}

/// Asks the portal which monitor or window to record, waits `delay`, then
/// starts encoding. Returns `None` if the user cancels the portal dialog.
pub async fn start(
    window: &gtk::ApplicationWindow,
    delay: Duration,
    language: Language,
    storage: Storage,
) -> Result<Option<Recording>, Box<dyn std::error::Error>> {
    let portal = Screencast::new().await?;
    let session = portal.create_session().await?;
    let result: Result<Option<(PathBuf, gio::Subprocess)>, Box<dyn std::error::Error>> = async {
        portal
            .select_sources(
                &session,
                CursorMode::Embedded,
                SourceType::Monitor | SourceType::Window,
                false,
                None,
                PersistMode::DoNot,
            )
            .await?
            .response()?;
        let parent = WindowIdentifier::from_native(window).await;
        let streams = match portal
            .start(&session, parent.as_ref())
            .await
            .and_then(|r| r.response())
        {
            Ok(streams) => streams,
            Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let node = streams
            .streams()
            .first()
            .ok_or(language.text(
                "Portal tidak memberikan layar untuk direkam.",
                "The portal did not provide a screen to record.",
            ))?
            .pipe_wire_node_id();
        let fd = portal.open_pipe_wire_remote(&session).await?;
        window.minimize();
        glib::timeout_future(delay).await;
        let path = output_path(storage).ok_or(language.text(
            "Folder video tidak dapat dibuat.",
            "The video folder could not be created.",
        ))?;
        match spawn(fd, node, &path) {
            Ok(process) => Ok(Some((path, process))),
            Err(error) => Err(format!(
                "{} ({error})",
                language.text(
                    "Perekam GStreamer tidak dapat dijalankan. Pasang gstreamer1, gstreamer1-plugins-good, dan pipewire-gstreamer.",
                    "The GStreamer recorder could not start. Install gstreamer1, gstreamer1-plugins-good, and pipewire-gstreamer.",
                )
            )
            .into()),
        }
    }
    .await;
    match result {
        Ok(Some((path, process))) => Ok(Some(Recording {
            path,
            process,
            session,
            _portal: portal,
        })),
        other => {
            let _ = session.close().await;
            other.map(|_| None)
        }
    }
}

fn output_path(storage: Storage) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    // The owner of /proc/self is the user running this process.
    let uid = fs::metadata("/proc/self").ok()?.uid();
    let dir = storage_dir(storage, uid, glib::UserDirectory::Videos, "Screencasts")?;
    unique_path(&dir, "Screencast", "webm")
}

fn spawn(fd: OwnedFd, node: u32, path: &std::path::Path) -> Result<gio::Subprocess, glib::Error> {
    // gst-launch joins its arguments and parses them again, so quote the path.
    let location = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let pipeline = [
        "pipewiresrc".into(),
        "fd=0".into(),
        format!("path={node}"),
        "do-timestamp=true".into(),
        "keepalive-time=1000".into(),
        // Finish the file normally when the user stops sharing in GNOME.
        "on-disconnect=eos".into(),
        "!".into(),
        "videoconvert".into(),
        "!".into(),
        "queue".into(),
        "!".into(),
        "vp8enc".into(),
        "cpu-used=16".into(),
        "deadline=1".into(),
        "threads=4".into(),
        "min-quantizer=4".into(),
        "max-quantizer=20".into(),
        "!".into(),
        "queue".into(),
        "!".into(),
        "webmmux".into(),
        "!".into(),
        "filesink".into(),
        format!("location=\"{location}\""),
    ];
    let mut argv: Vec<&std::ffi::OsStr> = ["gst-launch-1.0", "-e", "-q"]
        .iter()
        .map(std::ffi::OsStr::new)
        .collect();
    argv.extend(pipeline.iter().map(std::ffi::OsStr::new));
    let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDERR_PIPE);
    launcher.take_stdin_fd(Some(fd));
    launcher.spawn(&argv)
}

impl Recording {
    /// Asks the encoder to finish the file; `wait` then returns.
    pub fn stop(process: &gio::Subprocess) {
        process.send_signal(SIGINT);
    }

    /// Waits until encoding ends and returns the saved video.
    pub async fn wait(self, language: Language) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let output = self.process.communicate_utf8_future(None).await;
        let _ = self.session.close().await;
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > 0) {
            return Ok(self.path);
        }
        let _ = fs::remove_file(&self.path);
        let detail = output
            .ok()
            .and_then(|(_, stderr)| stderr)
            .and_then(|s| {
                s.lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        Err(format!(
            "{} {detail}",
            language.text("Video kosong.", "The video is empty.")
        )
        .trim_end()
        .into())
    }
}
