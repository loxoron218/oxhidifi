//! Filesystem watcher events and read-only event filtering.
//!
//! Maps raw notify results to [`WatcherEvent`]s, dropping read-only events
//! (opens, closes, `atime` touches) so file reads never trigger incremental
//! scans. Also resolves event paths to scan targets for debouncing and burst
//! coalescing.

use std::path::{Path, PathBuf};

use {
    notify::{
        Error, Event,
        EventKind::{self, Modify},
        event::{MetadataKind::AccessTime, ModifyKind::Metadata},
    },
    tokio::sync::mpsc::UnboundedSender,
    tracing::warn,
};

/// Events emitted by the filesystem watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    /// A directory was modified (files added/removed/changed).
    DirectoryModified {
        /// Path of the modified directory.
        path: PathBuf,
    },
    /// An error occurred during watching.
    Error {
        /// Error message.
        error: String,
    },
}

/// Report whether a notify event kind can never require a rescan.
///
/// Playback, cover decoding, and scan hashing all read audio files,
/// emitting `Access` events; `atime`-only metadata touches change no
/// content. Forwarding those as `DirectoryModified` caused
/// self-perpetuating scan storms (each scan's reads triggering the next
/// scan and starving cover decodes).
///
/// # Arguments
///
/// * `kind` - Notify event kind to classify.
///
/// # Returns
///
/// `true` when the event is read-only and must be ignored.
#[must_use]
pub fn is_ignorable_kind(kind: &EventKind) -> bool {
    if kind.is_access() || kind.is_other() {
        return true;
    }
    matches!(kind, Modify(Metadata(AccessTime)))
}

/// Handle a raw watcher event and forward it through the channel.
///
/// Read-only events (opens, closes, `atime` touches) are dropped here so
/// file reads never trigger incremental scans.
pub fn handle_watcher_event(
    result: Result<Event, Error>,
    event_tx: &UnboundedSender<WatcherEvent>,
) {
    let Some(event) = watcher_event_for(result) else {
        return;
    };
    if event_tx.is_closed() {
        return;
    }
    if let Err(e) = event_tx.send(event) {
        warn!(error = %e, "Failed to send watcher event");
    }
}

/// Map a raw notify result to a watcher event.
///
/// # Arguments
///
/// * `result` - Raw notify callback result.
///
/// # Returns
///
/// `None` for read-only events that must never trigger scans, otherwise
/// the forwarded watcher event.
fn watcher_event_for(result: Result<Event, Error>) -> Option<WatcherEvent> {
    match result {
        Ok(event) if is_ignorable_kind(&event.kind) => None,
        Ok(event) => Some(WatcherEvent::DirectoryModified {
            path: event.paths.first().cloned().unwrap_or_default(),
        }),
        Err(e) => Some(WatcherEvent::Error {
            error: e.to_string(),
        }),
    }
}

/// Resolve the scan target for a watcher event path.
///
/// # Arguments
///
/// * `path` - Raw event path (file, directory, or removed path).
///
/// # Returns
///
/// The parent directory for existing files (one album copy collapses to one
/// parent scan), otherwise the path itself (directories scan directly,
/// removals resolve by the stored path).
pub fn scan_target_for(path: &Path) -> PathBuf {
    if path.is_file() {
        path.parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use {
        anyhow::{Result, ensure},
        notify::{
            Error,
            ErrorKind::Generic,
            Event,
            EventKind::{Access, Create, Modify},
            event::{
                AccessKind::Read,
                CreateKind::File,
                DataChange::Any,
                MetadataKind::AccessTime,
                ModifyKind::{Data, Metadata},
            },
        },
        tokio::sync::mpsc::unbounded_channel,
    };

    use crate::library::watcher_event::{
        WatcherEvent::{DirectoryModified, Error as WatcherError},
        handle_watcher_event, is_ignorable_kind, scan_target_for,
    };

    #[test]
    fn watcher_event_clone() {
        let event = DirectoryModified {
            path: PathBuf::from("/music"),
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    #[test]
    fn ignorable_kinds_cover_reads_and_atime() {
        assert!(
            is_ignorable_kind(&Access(Read)),
            "file reads must never trigger scans"
        );
        assert!(
            is_ignorable_kind(&Modify(Metadata(AccessTime))),
            "atime touches must never trigger scans"
        );
        assert!(
            !is_ignorable_kind(&Create(File)),
            "file creation must trigger scans"
        );
        assert!(
            !is_ignorable_kind(&Modify(Data(Any))),
            "data changes must trigger scans"
        );
    }

    #[test]
    fn scan_target_resolves_parent_for_files() {
        assert_eq!(
            scan_target_for(Path::new("src/lib.rs")),
            Path::new("src"),
            "a file event must resolve to its parent directory"
        );
        assert_eq!(
            scan_target_for(Path::new("src")),
            Path::new("src"),
            "a directory must resolve to itself"
        );
        assert_eq!(
            scan_target_for(Path::new("src/no-such-oxhidifi-file.flac")),
            Path::new("src/no-such-oxhidifi-file.flac"),
            "a removed path must resolve to itself"
        );
    }

    #[test]
    fn handle_watcher_event_forwards_modified() {
        let (tx, mut rx) = unbounded_channel();
        let mut event = Event::new(Create(File));
        event.paths.push(PathBuf::from("/music/new.flac"));
        handle_watcher_event(Ok(event), &tx);
        assert_eq!(
            rx.try_recv(),
            Ok(DirectoryModified {
                path: PathBuf::from("/music/new.flac"),
            })
        );
    }

    #[test]
    fn handle_watcher_event_ignores_access() -> Result<()> {
        let (tx, mut rx) = unbounded_channel();
        let mut event = Event::new(Access(Read));
        event.paths.push(PathBuf::from("/music/played.flac"));
        handle_watcher_event(Ok(event), &tx);
        ensure!(
            rx.try_recv().is_err(),
            "access events must be dropped before the scan queue"
        );
        Ok(())
    }

    #[test]
    fn handle_watcher_event_forwards_error() {
        let (tx, mut rx) = unbounded_channel();
        let err = Error::new(Generic("watcher failure".to_string()));
        handle_watcher_event(Err(err), &tx);
        assert_eq!(
            rx.try_recv(),
            Ok(WatcherError {
                error: "watcher failure".to_string(),
            })
        );
    }
}
