use crate::document::Document;
use ashpd::WindowIdentifier;
use ashpd::desktop::{ResponseError, screenshot::Screenshot};
use gtk::{gdk_pixbuf::Pixbuf, gio, glib, prelude::*};
use std::time::{Duration, Instant};

async fn request(
    interactive: bool,
    parent: Option<WindowIdentifier>,
) -> Result<Option<Screenshot>, ashpd::Error> {
    match Screenshot::request()
        .identifier(parent)
        .interactive(interactive)
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
    interactive: bool,
    delay: Duration,
    language: crate::settings::Language,
) -> Result<Option<Document>, Box<dyn std::error::Error>> {
    let capture_hidden = async || {
        window.set_visible(false);
        glib::timeout_future(delay).await;
        request(interactive, None).await
    };
    let shot = match capture_hidden().await {
        Err(ashpd::Error::Response(ResponseError::Other)) if !interactive => {
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
            if request(false, Some(parent)).await?.is_none() {
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
    Ok(Some(Document::from_image(&image)?))
}
