use std::{fs, path::Path};

use sempre_converter::Source;
use sempre_manager::{Manager, ManagerError};
use sempre_state::Store;
use sempre_subscription::SubscriptionError;
use serde_json::{Map, json};
use uuid::Uuid;

use crate::ClientError;

const MAX_SUBSCRIPTION_ARGUMENT_SIZE: usize = 64 << 10;

pub(crate) struct Options<'a> {
    pub core: Option<&'a str>,
    pub subscription: Option<&'a str>,
    pub subscription_file: Option<&'a Path>,
    pub ui: Option<&'a str>,
    pub ui_sha256: Option<&'a str>,
}

pub(crate) async fn prepare(manager: &Manager, options: Options<'_>) -> Result<(), ClientError> {
    if let Some(core) = options.core {
        select_or_install(manager, core).await?;
    }
    let subscription = match (options.subscription, options.subscription_file) {
        (Some(value), None) => Some(value.trim().to_owned()),
        (None, Some(path)) => Some(read_subscription(path)?),
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("clap rejects conflicting subscription options"),
    };
    if let Some(value) = subscription {
        configure_subscription(manager, &value).await?;
    }
    if let Some(source) = options.ui {
        install_ui(manager.store(), source, options.ui_sha256.unwrap_or("")).await?;
    }
    Ok(())
}

async fn select_or_install(manager: &Manager, reference: &str) -> Result<(), ClientError> {
    match manager.select_core(reference).await {
        Ok(_) => Ok(()),
        Err(ManagerError::NotInstalled(_)) => {
            manager.install_core(reference).await?;
            manager.select_core(reference).await?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

async fn configure_subscription(manager: &Manager, value: &str) -> Result<(), ClientError> {
    if value.is_empty() {
        return Err(SubscriptionError::Invalid("subscription cannot be empty".into()).into());
    }
    let source = source(value);
    let mut profile_id = String::new();
    manager.subscriptions().update(|catalog| {
        let profile = catalog.profiles.first_mut().ok_or_else(|| {
            SubscriptionError::Invalid("default subscription profile is unavailable".into())
        })?;
        profile.sources = vec![source];
        profile.revision += 1;
        profile_id.clone_from(&profile.id);
        Ok(())
    })?;
    manager.activate_subscription_profile(&profile_id).await?;
    Ok(())
}

fn source(value: &str) -> Source {
    let is_url = url::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    });
    let mut extra = Map::new();
    extra.insert("fetch_mode".into(), json!("auto"));
    Source {
        id: Uuid::new_v4().to_string(),
        kind: if is_url { "url" } else { "raw" }.into(),
        enabled: true,
        url: if is_url { value.into() } else { String::new() },
        remark: "Initial subscription".into(),
        prefix: String::new(),
        content: if is_url { String::new() } else { value.into() },
        user_agent: "clash.meta".into(),
        extra,
    }
}

fn read_subscription(path: &Path) -> Result<String, ClientError> {
    let metadata = fs::metadata(path).map_err(|source| ClientError::Io {
        operation: "inspect subscription argument file",
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_SUBSCRIPTION_ARGUMENT_SIZE as u64
    {
        return Err(SubscriptionError::Invalid(format!(
            "subscription argument file must be non-empty and at most {MAX_SUBSCRIPTION_ARGUMENT_SIZE} bytes"
        ))
        .into());
    }
    let data = fs::read(path).map_err(|source| ClientError::Io {
        operation: "read subscription argument file",
        path: path.to_path_buf(),
        source,
    })?;
    let value = String::from_utf8(data)
        .map_err(|_| SubscriptionError::Invalid("subscription argument must be UTF-8".into()))?;
    let value = value.trim_start_matches('\u{feff}').trim().to_owned();
    if value.is_empty() {
        return Err(SubscriptionError::Invalid("subscription argument is empty".into()).into());
    }
    Ok(value)
}

async fn install_ui(store: &Store, source: &str, expected_digest: &str) -> Result<(), ClientError> {
    if matches!(source, "official" | "bundled") {
        if !expected_digest.is_empty() {
            return Err(
                sempre_ui::UiError::Invalid("--ui-sha256 requires an HTTPS UI URL".into()).into(),
            );
        }
        sempre_ui::Store::new(&store.layout().ui).current()?;
        return Ok(());
    }
    let ui = sempre_ui::Store::new(&store.layout().ui);
    if source.starts_with("https://") {
        ui.install_url(source, "url", source, expected_digest)
            .await?;
    } else {
        if !expected_digest.is_empty() {
            return Err(sempre_ui::UiError::Invalid(
                "--ui-sha256 is not accepted for GitHub UI references".into(),
            )
            .into());
        }
        ui.install_github(source).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_argument_detects_urls_and_raw_content() {
        let url = source("https://example.com/subscription?token=secret");
        assert_eq!(url.kind, "url");
        assert!(url.content.is_empty());
        let raw = source("ss://example");
        assert_eq!(raw.kind, "raw");
        assert_eq!(raw.content, "ss://example");
    }

    #[test]
    fn subscription_file_is_bounded_trimmed_utf8() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("subscription.txt");
        fs::write(&path, "\u{feff} https://example.com/sub \n").expect("subscription file");
        assert_eq!(
            read_subscription(&path).expect("subscription"),
            "https://example.com/sub"
        );
        fs::write(&path, vec![b'a'; MAX_SUBSCRIPTION_ARGUMENT_SIZE + 1])
            .expect("oversized subscription file");
        assert!(read_subscription(&path).is_err());
    }
}
