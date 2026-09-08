//! Cover-art decode helpers that wire decoded images into detail-page `Picture` widgets.

use std::sync::Arc;

use {
    async_channel::{
        Receiver, Sender,
        TryRecvError::{Closed, Empty},
        unbounded,
    },
    libadwaita::{
        glib::{
            ControlFlow::{self, Break, Continue},
            idle_add_local,
        },
        gtk::Picture,
    },
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    ui::{
        image_decode::{DecodedCover, raw_to_texture},
        texture_pool::dispatch::ArtworkDecodeRequest,
    },
};

/// Try to send decoded cover to the main thread channel, logging on failure.
pub fn try_send_cover(tx: &Sender<DecodedCover>, decoded: Option<DecodedCover>) {
    let Some(decoded) = decoded else { return };
    if let Err(e) = tx.try_send(decoded) {
        error!(error = %e, "Failed to send decoded detail cover to main thread");
    }
}

/// Poll for decoded artwork and apply it to the picture widget.
#[must_use]
pub fn poll_artwork(rx: &Receiver<DecodedCover>, artwork: &Picture) -> ControlFlow {
    match rx.try_recv() {
        Ok(decoded) => {
            let texture = raw_to_texture(&decoded);
            artwork.set_paintable(Some(&texture));
            Break
        }
        Err(Empty) => Continue,
        Err(Closed) => Break,
    }
}

/// Request cover decode for a detail page and wire the result into `picture`.
///
/// The decode runs off the main thread; the decoded cover is applied via an
/// idle callback.
pub fn decode_cover_into_picture(
    state: &Arc<AppState>,
    album_id: i64,
    path: String,
    size: i32,
    picture: &Picture,
) {
    let (tx, rx) = unbounded::<DecodedCover>();
    state.cover_art_cache.request_decode(ArtworkDecodeRequest {
        album_id,
        path,
        size,
        on_complete: Box::new(move |_, decoded| try_send_cover(&tx, decoded)),
    });
    let picture = picture.clone();
    state
        .handles
        .lock()
        .retain_source(idle_add_local(move || poll_artwork(&rx, &picture)));
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, anyhow, ensure},
        async_channel::unbounded,
        libadwaita::{
            glib::ControlFlow::{Break, Continue},
            gtk::{self, Picture, test},
        },
    };

    use crate::ui::{
        detail::cover_art::{poll_artwork, try_send_cover},
        image_decode::DecodedCover,
        texture_pool::dispatch::tests::{
            cover_send_forwards_decoded, cover_send_none_is_noop, mock_decoded_cover,
        },
    };

    #[test]
    fn try_send_cover_none_is_noop() -> Result<()> {
        cover_send_none_is_noop(try_send_cover)
    }

    #[test]
    fn try_send_cover_forwards_decoded() -> Result<()> {
        cover_send_forwards_decoded(try_send_cover)
    }

    #[test]
    fn poll_artwork_breaks_on_decoded_cover() -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        let artwork = Picture::new();
        tx.try_send(mock_decoded_cover())
            .map_err(|e| anyhow!("{e}"))?;
        ensure!(poll_artwork(&rx, &artwork) == Break);
        Ok(())
    }

    #[test]
    fn poll_artwork_continues_while_waiting() -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        let artwork = Picture::new();
        ensure!(poll_artwork(&rx, &artwork) == Continue);
        drop(tx);
        Ok(())
    }

    #[test]
    fn poll_artwork_breaks_on_closed_channel() -> Result<()> {
        let (tx, rx) = unbounded::<DecodedCover>();
        drop(tx);
        let artwork = Picture::new();
        ensure!(poll_artwork(&rx, &artwork) == Break);
        Ok(())
    }
}
