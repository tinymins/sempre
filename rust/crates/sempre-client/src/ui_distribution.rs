use std::path::Path;

use sempre_state::Layout;

pub(crate) async fn install(
    layout: &Layout,
    source: &str,
    digest: &str,
) -> Result<sempre_ui::Metadata, String> {
    let source = source.trim();
    if source.is_empty() || source == "official" {
        return install_official(layout).await;
    }
    let store = sempre_ui::Store::new(&layout.ui);
    if Path::new(source).is_file() {
        return store
            .install_file(Path::new(source), "local", source, digest)
            .map_err(|error| error.to_string());
    }
    if source.starts_with("https://") {
        return store
            .install_url(source, "url", source, digest)
            .await
            .map_err(|error| error.to_string());
    }
    store
        .install_github(source)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn update(layout: &Layout) -> Result<sempre_ui::Metadata, String> {
    let store = sempre_ui::Store::new(&layout.ui);
    let current = store.current().map_err(|error| error.to_string())?;
    match current.source_type.as_str() {
        "official" => install_official(layout).await,
        "github" => store
            .install_github(&current.source)
            .await
            .map_err(|error| error.to_string()),
        "url" => store
            .install_url(&current.source, "url", &current.source, "")
            .await
            .map_err(|error| error.to_string()),
        "local" => Err("locally installed UI has no update source; install another archive".into()),
        value => Err(format!("unsupported UI source type {value:?}")),
    }
}

pub(crate) async fn install_official(layout: &Layout) -> Result<sempre_ui::Metadata, String> {
    let archive = layout.resources.join("sempre-ui.zip");
    if archive.is_file() {
        let digest = checksum(&layout.resources.join("SHA256SUMS"), "sempre-ui.zip")?;
        let store = sempre_ui::Store::new(&layout.ui);
        return tokio::task::spawn_blocking(move || {
            store.install_file(&archive, "official", "bundle", &digest)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string());
    }
    let releases =
        sempre_artifact::GithubClient::new(concat!("Sempre/", env!("CARGO_PKG_VERSION")))
            .map_err(|error| error.to_string())?;
    let release = releases
        .release("tinymins/sempre", "stable")
        .await
        .map_err(|error| error.to_string())?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == "sempre-ui.zip")
        .ok_or_else(|| format!("release {} has no sempre-ui.zip", release.tag))?;
    let digest = asset
        .digest
        .parse::<sempre_artifact::Sha256Digest>()
        .map_err(|_| "official UI release asset has no valid SHA-256 digest".to_owned())?;
    sempre_ui::Store::new(&layout.ui)
        .install_url(&asset.url, "official", &asset.url, &digest.to_string())
        .await
        .map_err(|error| error.to_string())
}

fn checksum(path: &Path, name: &str) -> Result<String, String> {
    let data = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    data.lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let candidate = fields.next()?.trim_start_matches('*');
            (candidate == name
                && digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| digest.to_owned())
        })
        .ok_or_else(|| format!("{name} is absent from or invalid in SHA256SUMS"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_checksum_requires_an_exact_named_sha256() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("SHA256SUMS");
        let digest = "a".repeat(64);
        std::fs::write(&path, format!("{digest}  sempre-ui.zip\n")).expect("checksums");
        assert_eq!(checksum(&path, "sempre-ui.zip").expect("digest"), digest);
        assert!(checksum(&path, "missing.zip").is_err());
    }
}
