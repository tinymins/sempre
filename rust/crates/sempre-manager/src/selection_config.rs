use sempre_converter::Profile;
use sempre_core::CoreRef;
use sempre_state::{ConfigBuild, Document};

use crate::{
    Manager, ManagerError, ValidationRunner, VersionRunner,
    config::PreparedConfig,
    subscription::{RenderedProfile, config_build, find_profile, record_compilation},
};

pub(crate) struct SelectionConfig {
    pub(crate) previous_hash: Option<String>,
    pub(crate) previous_build: Option<ConfigBuild>,
    pub(crate) candidate_hash: Option<String>,
    pub(crate) candidate_build: Option<ConfigBuild>,
    compilation: Option<Box<(Profile, RenderedProfile)>>,
    prepared: Option<PreparedConfig>,
}

impl SelectionConfig {
    pub(crate) fn discard(self) {
        if let Some(prepared) = self.prepared {
            prepared.discard();
        }
    }

    pub(crate) fn record<R: VersionRunner>(self, manager: &Manager<R>) -> Result<(), ManagerError> {
        if let Some(compilation) = self.compilation {
            let (original, rendered) = *compilation;
            record_compilation(
                &manager.subscriptions,
                &original,
                rendered.updated,
                &rendered.render,
                chrono::Utc::now(),
            )?;
        }
        Ok(())
    }
}

impl<R: VersionRunner + ValidationRunner> Manager<R> {
    pub(crate) async fn prepare_selection_config(
        &self,
        document: &Document,
        reference: &CoreRef,
        version: &str,
    ) -> Result<SelectionConfig, ManagerError> {
        let previous_hash = document.configs.get(&reference.core).cloned();
        let previous_build = document.config_builds.get(&reference.core).cloned();
        let expected_build = self.active_subscription_build_for(document, reference, version)?;
        let mut candidate_hash = previous_hash.clone();
        let mut candidate_build = previous_build.clone();
        let mut compilation = None;
        let mut prepared = None;
        let mismatched =
            previous_hash.is_some() && expected_build.as_ref() != previous_build.as_ref();
        if mismatched
            && let Some((original, rendered)) =
                Box::pin(self.render_active_subscription_for(document, reference, version)).await?
        {
            let compilation_item = Box::new((original, rendered));
            let build = config_build(&compilation_item.1.updated, &compilation_item.1.target)?;
            let candidate = self
                .prepare_config_content_for(
                    reference,
                    version,
                    compilation_item.1.render.content.as_bytes(),
                )
                .await
                .map_err(|source| ManagerError::CandidateRejected {
                    reference: reference.to_string(),
                    source: Box::new(source),
                })?;
            candidate_hash = Some(candidate.hash.clone());
            candidate_build = Some(build);
            compilation = Some(compilation_item);
            prepared = Some(candidate);
        } else if let Some(hash) = &previous_hash {
            let config = self.store.layout().config(&reference.core, hash);
            self.validate_config_path(reference, version, &config)
                .await
                .map_err(|source| ManagerError::CandidateRejected {
                    reference: reference.to_string(),
                    source: Box::new(source),
                })?;
        }
        Ok(SelectionConfig {
            previous_hash,
            previous_build,
            candidate_hash,
            candidate_build,
            compilation,
            prepared,
        })
    }

    pub(crate) fn active_subscription_build_for(
        &self,
        document: &Document,
        reference: &CoreRef,
        version: &str,
    ) -> Result<Option<ConfigBuild>, ManagerError> {
        let Some(id) = document.active_profile_id.as_deref() else {
            return Ok(None);
        };
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?;
        let (target, _) = self.subscription_target_for(reference, version)?;
        Ok(Some(config_build(profile, &target)?))
    }

    pub(crate) async fn render_active_subscription_for(
        &self,
        document: &Document,
        reference: &CoreRef,
        version: &str,
    ) -> Result<Option<(Profile, RenderedProfile)>, ManagerError> {
        let Some(id) = document.active_profile_id.as_deref() else {
            return Ok(None);
        };
        let catalog = self.subscriptions.read()?;
        let profile = find_profile(&catalog, id)?.clone();
        let (target, mut warnings) = self.subscription_target_for(reference, version)?;
        let rendered = self
            .render_subscription_for_target(&catalog, &profile, target, &mut warnings, false)
            .await?;
        Ok(Some((profile, rendered)))
    }
}
