use chrono::Utc;
use sempre_state::{Document, Installation, StateValidationError};

fn installation() -> Installation {
    Installation {
        explicit: true,
        digest: "a".repeat(64),
        source: "test".into(),
        installed_at: Utc::now(),
    }
}

#[test]
fn repositories_cannot_escape_the_core_directory() {
    let mut document = Document::default();
    document
        .core_mut("sing-box")
        .source_mut(Some("tinymins/.."))
        .installed
        .insert("1.2.3".into(), installation());
    assert!(matches!(
        document.validate(),
        Err(StateValidationError::Repository(_))
    ));
}

#[test]
fn versions_cannot_escape_the_core_directory() {
    let mut document = Document::default();
    document
        .core_mut("sing-box")
        .source_mut(None)
        .installed
        .insert("1.2.3-../../escape".into(), installation());
    assert!(matches!(
        document.validate(),
        Err(StateValidationError::Version(_))
    ));
}
