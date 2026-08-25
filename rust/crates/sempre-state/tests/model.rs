use chrono::Utc;
use sempre_state::{Deployment, Document, Installation, Selection, StateValidationError};

fn installation() -> Installation {
    Installation {
        explicit: true,
        digest: "a".repeat(64),
        source: "test".into(),
        installed_at: Utc::now(),
    }
}

#[test]
fn default_document_is_valid() {
    Document::default().validate().expect("default state");
}

#[test]
fn selection_must_reference_an_installed_version() {
    let mut document = Document {
        selected: Some(Selection {
            core: "sing-box".into(),
            repository: None,
            reference: "1.2.3".into(),
        }),
        ..Document::default()
    };
    assert_eq!(
        document.validate(),
        Err(StateValidationError::MissingCore("sing-box".into()))
    );
    document
        .core_mut("sing-box")
        .source_mut(None)
        .installed
        .insert("1.2.3".into(), installation());
    document.validate().expect("installed selection");
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

#[test]
fn subscription_requires_https_without_credentials() {
    let mut document = Document::default();
    document.subscription.url = Some("https://user@example.com/subscription".into());
    assert_eq!(
        document.validate(),
        Err(StateValidationError::SubscriptionUrl)
    );
}

#[test]
fn staging_preserves_the_last_confirmed_deployment() {
    let mut document = Document::default();
    let first = Deployment {
        core: "sing-box".into(),
        repository: None,
        reference: "1.2.3".into(),
        version: "1.2.3".into(),
        config_hash: "a".repeat(64),
    };
    let second = Deployment {
        version: "1.2.4".into(),
        reference: "1.2.4".into(),
        config_hash: "b".repeat(64),
        ..first.clone()
    };
    document.active = Some(first.clone());
    document.stage(second.clone());
    assert_eq!(document.previous, Some(first));
    assert_eq!(document.active, Some(second));
    assert!(document.pending);
}
