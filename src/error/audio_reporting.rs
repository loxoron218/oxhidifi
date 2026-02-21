//! Domain error handling utilities.
//!
//! This module provides helper functions for common error handling patterns
//! across the application, particularly for audio-related errors.

use crate::{
    audio::{engine::AudioError, output::OutputError::ExclusiveModeFailed},
    state::AppState,
};

/// Checks if an audio error is an exclusive mode failure and reports it.
///
/// # Arguments
///
/// * `error` - The audio error to check
/// * `app_state` - Application state for reporting
///
/// # Returns
///
/// `true` if the error was an exclusive mode failure, `false` otherwise.
#[must_use]
pub fn handle_exclusive_mode_error(error: &AudioError, app_state: &AppState) -> bool {
    if let AudioError::OutputError(ExclusiveModeFailed { reason }) = error {
        app_state.report_exclusive_mode_failure(reason.clone());
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use {
        anyhow::{Result, bail},
        parking_lot::RwLock,
    };

    use crate::{
        audio::{
            decoder_types::DecoderError::NoAudioTrack, engine::AudioError,
            output::OutputError::ExclusiveModeFailed,
        },
        config::SettingsManager,
        error::audio_reporting::handle_exclusive_mode_error,
        state::AppState,
    };

    fn create_test_app_state() -> Result<AppState> {
        let settings_manager = SettingsManager::new()?;
        Ok(AppState::new(
            Weak::new(),
            None,
            Arc::new(RwLock::new(settings_manager)),
        ))
    }

    #[test]
    fn test_handle_exclusive_mode_error_true() -> Result<()> {
        let reason = "Device busy".to_string();
        let error = AudioError::OutputError(ExclusiveModeFailed { reason });

        let app_state = create_test_app_state()?;
        let result = handle_exclusive_mode_error(&error, &app_state);
        if !result {
            bail!("Expected true");
        }
        Ok(())
    }

    #[test]
    fn test_handle_exclusive_mode_error_false() -> Result<()> {
        let decoder_error = NoAudioTrack;
        let error = AudioError::DecoderError(decoder_error);

        let app_state = create_test_app_state()?;
        let result = handle_exclusive_mode_error(&error, &app_state);
        if result {
            bail!("Expected false");
        }
        Ok(())
    }
}
