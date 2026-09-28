//! Shared describe contract check for stage wording.
//!
//! Single owner for the title/detail/explanation assertions used by the unit
//! test in [`stage_describe`](crate::playback::signal_path::stage_describe)
//! and the integration test target so copy-paste clones stay absent.

use crate::playback::signal_path::{
    StageFacts::{self, ExternalRenderer},
    stage_describe::describe,
};

/// Check one describe contract pair for non-empty text and expected wording.
///
/// Verifies the title, detail, and explanation are non-empty, the detail or
/// explanation carries `needle`, converters keep input-to-output wording, and
/// external renderers stay title-only.
///
/// # Arguments
///
/// * `kind` - Stage facts to describe.
/// * `needle` - Expected substring in the detail or explanation.
///
/// # Returns
///
/// * `()` - Contract holds for this pair.
///
/// # Errors
///
/// Returns an error string when any text is empty, the needle is missing, a
/// converter drops input-to-output wording, or a renderer title diverges.
pub fn check_describe_pair(kind: &StageFacts, needle: &str) -> Result<(), String> {
    let (title, detail, explanation) = describe(kind);
    let texts = [title.as_str(), detail.as_str(), explanation.as_str()];
    if !texts.iter().all(|text| !text.is_empty()) {
        return Err(String::from("empty"));
    }
    let hit = detail.contains(needle) || explanation.contains(needle);
    if !hit {
        return Err(format!("missing {needle}"));
    }
    if needle.contains(" to ") && !detail.contains(" to ") {
        return Err(String::from("converters need io"));
    }
    let renderer = matches!(kind, ExternalRenderer { .. });
    if renderer && title != detail {
        return Err(String::from("renderer stays title-only"));
    }
    Ok(())
}
