use crate::document::Document;
use crate::settings::Storage;
use ashpd::WindowIdentifier;
use ashpd::desktop::{ResponseError, screenshot::Screenshot};
use gtk::{gdk_pixbuf::Pixbuf, gio, glib, prelude::*};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

async fn request(parent: Option<WindowIdentifier>) -> Result<Option<Screenshot>, ashpd::Error> {
    match Screenshot::request()
        .identifier(parent)
        .interactive(false)
        .modal(true)
        .send()
        .await
        .and_then(|r| r.response())
    {
        Ok(shot) => Ok(Some(shot)),
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub async fn capture(
    window: &gtk::ApplicationWindow,
    delay: Duration,
    language: crate::settings::Language,
    storage: Storage,
) -> Result<Option<Document>, Box<dyn std::error::Error>> {
    let capture_hidden = async || {
        window.set_visible(false);
        glib::timeout_future(delay).await;
        request(None).await
    };
    let shot = match capture_hidden().await {
        Err(ashpd::Error::Response(ResponseError::Other)) => {
            // GNOME needs a focused parent for its first-use permission dialog.
            window.present();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !window.is_active() {
                if Instant::now() >= deadline {
                    return Err(language
                        .text(
                            "Aktifkan jendela Easy SS untuk meminta izin screenshot.",
                            "Focus the Easy SS window to request screenshot permission.",
                        )
                        .into());
                }
                glib::timeout_future(Duration::from_millis(50)).await;
            }
            let parent = WindowIdentifier::from_native(window)
                .await
                .ok_or(language.text(
                    "Portal gagal mengenali jendela Easy SS.",
                    "The portal could not identify the Easy SS window.",
                ))?;
            if request(Some(parent)).await?.is_none() {
                return Ok(None);
            }
            capture_hidden().await?
        }
        result => result?,
    };
    let Some(shot) = shot else { return Ok(None) };
    let file = gio::File::for_uri(shot.uri().as_str());
    let stream = file.read_future(gtk::glib::Priority::DEFAULT).await?;
    let image = Pixbuf::from_stream_future(&stream).await?;
    drop(stream);
    if let Some(path) = file.path() {
        relocate(&path, storage);
    }
    Ok(Some(Document::from_image(&image)?))
}

/// The portal always saves into ~/Pictures. Move the file to the folder the
/// user chose; on any failure it stays where the portal put it.
fn relocate(path: &Path, storage: Storage) {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let dir = match storage {
        Storage::Temporary => {
            // The portal writes the file as the current user.
            let Ok(uid) = fs::metadata(path).map(|m| m.uid()) else {
                return;
            };
            let dir = glib::tmp_dir().join(format!("easy-ss-{uid}"));
            let _ = fs::DirBuilder::new().mode(0o700).create(&dir);
            // Refuse a folder another user created in the shared /tmp.
            let Ok(meta) = fs::symlink_metadata(&dir) else {
                return;
            };
            if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
                return;
            }
            dir
        }
        Storage::Screenshots => {
            let Some(pictures) = glib::user_special_dir(glib::UserDirectory::Pictures) else {
                return;
            };
            let dir = pictures.join("Screenshots");
            if fs::create_dir_all(&dir).is_err() {
                return;
            }
            dir
        }
    };
    let stamp = glib::DateTime::now_local()
        .and_then(|now| now.format("%Y-%m-%d %H-%M-%S"))
        .map_or_else(|_| "capture".into(), |s| s.to_string());
    let Some(target) = (1..1000)
        .map(|n| match n {
            1 => dir.join(format!("Screenshot From {stamp}.png")),
            n => dir.join(format!("Screenshot From {stamp} ({n}).png")),
        })
        .find(|target| !target.exists())
    else {
        return;
    };
    if fs::rename(path, &target).is_ok()
        || fs::copy(path, &target).is_ok() && fs::remove_file(path).is_ok()
    {
        return;
    }
    let _ = fs::remove_file(&target);
}
