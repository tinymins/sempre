use sempre_converter::Profile;
use sempre_manager::{CoreChange, Manager};
use sempre_state::{Layout, Mode, Store};
use sempre_subscription::{SubscriptionError, new_profile};
use serde::Serialize;
use serde_json::json;
use url::Url;

use crate::{ClientError, args::SubscriptionCommand};

pub(crate) async fn run(
    mode: Mode,
    command: SubscriptionCommand,
    json_output: bool,
) -> Result<(), ClientError> {
    let manager = Manager::new(Store::new(Layout::for_mode(mode)?))?;
    match command {
        SubscriptionCommand::List => list(&manager, json_output),
        SubscriptionCommand::Show { id } => show(&manager, id.as_deref(), json_output),
        SubscriptionCommand::Create { name } => create(&manager, &name, None, json_output),
        SubscriptionCommand::CreateRemote { name, manifest_url } => {
            create(&manager, &name, Some(&manifest_url), json_output)
        }
        SubscriptionCommand::Use { id } => {
            let (change, render) = manager.activate_subscription_profile(&id).await?;
            output_prepare(&change, &render, json_output)
        }
        SubscriptionCommand::Update { id } => {
            let id = resolve_profile_id(&manager, id.as_deref())?;
            let (change, render) = manager.refresh_subscription_profile(&id).await?;
            output_prepare(&change, &render, json_output)
        }
        SubscriptionCommand::Render { id, format } => {
            let id = resolve_profile_id(&manager, id.as_deref())?;
            let render = manager
                .render_subscription_profile(&id, &format, true)
                .await?;
            if json_output {
                print_json(&render)
            } else {
                println!("{}", render.content);
                Ok(())
            }
        }
        SubscriptionCommand::Remove { id } => remove(&manager, &id),
        SubscriptionCommand::Set { url } => {
            let change = manager.set_subscription_source(&url)?;
            output_change(&change, json_output)
        }
        SubscriptionCommand::Schedule { interval } => output_changes(
            &manager.update_subscription_settings(Some(&interval), None)?,
            json_output,
        ),
        SubscriptionCommand::AutoRestart { enabled } => output_changes(
            &manager.update_subscription_settings(None, Some(enabled))?,
            json_output,
        ),
        SubscriptionCommand::Status => status(&manager, json_output),
        SubscriptionCommand::ClearCache => {
            let change = manager.clear_subscription_cache()?;
            output_change(&change, json_output)
        }
    }
}

fn list(manager: &Manager, json_output: bool) -> Result<(), ClientError> {
    let catalog = manager.subscriptions().read()?;
    let active = manager.state()?.active_profile_id;
    if json_output {
        return print_json(&json!({ "profiles": catalog.profiles, "active_profile_id": active }));
    }
    for profile in catalog.profiles {
        let marker = if active.as_deref() == Some(&profile.id) {
            "*"
        } else {
            " "
        };
        println!(
            "{marker} {}\t{}\t{}",
            profile.id,
            profile_mode(&profile),
            profile.name
        );
    }
    Ok(())
}

fn show(manager: &Manager, id: Option<&str>, json_output: bool) -> Result<(), ClientError> {
    let id = resolve_profile_id(manager, id)?;
    let profile = find_profile(manager, &id)?;
    if json_output {
        print_json(&profile)
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&profile).map_err(ClientError::Json)?
        );
        Ok(())
    }
}

fn create(
    manager: &Manager,
    name: &str,
    manifest_url: Option<&str>,
    json_output: bool,
) -> Result<(), ClientError> {
    if name.trim().is_empty() {
        return Err(SubscriptionError::Invalid("profile name is required".into()).into());
    }
    let mut profile = new_profile(name);
    if let Some(manifest_url) = manifest_url {
        validate_remote_url(manifest_url)?;
        profile.extra.insert("mode".into(), json!("remote"));
        profile.extra.insert(
            "remote".into(),
            json!({ "manifest_url": manifest_url.trim() }),
        );
    }
    manager.subscriptions().update(|catalog| {
        catalog.profiles.push(profile.clone());
        Ok(())
    })?;
    if json_output {
        print_json(&profile)
    } else {
        println!("Profile created: {} ({})", profile.name, profile.id);
        Ok(())
    }
}

fn remove(manager: &Manager, id: &str) -> Result<(), ClientError> {
    if manager.state()?.active_profile_id.as_deref() == Some(id) {
        return Err(SubscriptionError::Invalid(
            "the active subscription profile cannot be deleted".into(),
        )
        .into());
    }
    manager.subscriptions().update(|catalog| {
        if catalog.profiles.len() == 1 {
            return Err(SubscriptionError::Invalid(
                "at least one subscription profile is required".into(),
            ));
        }
        let before = catalog.profiles.len();
        catalog.profiles.retain(|profile| profile.id != id);
        if before == catalog.profiles.len() {
            return Err(SubscriptionError::Invalid("profile was not found".into()));
        }
        Ok(())
    })?;
    println!("Profile removed: {id}");
    Ok(())
}

fn status(manager: &Manager, json_output: bool) -> Result<(), ClientError> {
    let state = manager.state()?;
    let id = resolve_profile_id(manager, None)?;
    let profile = find_profile(manager, &id)?;
    if json_output {
        return print_json(&json!({
            "profile": profile,
            "schedule": state.subscription,
            "auto_restart": state.subscription_auto_restart,
        }));
    }
    println!("Profile: {}", profile.name);
    println!("Profile ID: {}", profile.id);
    println!("Mode: {}", profile_mode(&profile));
    println!("Sources: {}", profile.sources.len());
    println!("Schedule: {}", state.subscription.interval);
    println!("Automatic restart: {}", state.subscription_auto_restart);
    if let Some(last_check) = state.subscription.last_check {
        println!("Last check: {last_check}");
    }
    if let Some(last_result) = state.subscription.last_result {
        println!("Last result: {last_result}");
    }
    Ok(())
}

fn resolve_profile_id(manager: &Manager, id: Option<&str>) -> Result<String, ClientError> {
    if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
        return Ok(id.into());
    }
    if let Some(id) = manager.state()?.active_profile_id {
        return Ok(id);
    }
    manager
        .subscriptions()
        .read()?
        .profiles
        .first()
        .map(|profile| profile.id.clone())
        .ok_or_else(|| SubscriptionError::Invalid("no subscription profile exists".into()).into())
}

fn find_profile(manager: &Manager, id: &str) -> Result<Profile, ClientError> {
    manager
        .subscriptions()
        .read()?
        .profiles
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| SubscriptionError::Invalid("profile was not found".into()).into())
}

fn validate_remote_url(value: &str) -> Result<(), ClientError> {
    let valid = Url::parse(value.trim()).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    });
    if valid {
        Ok(())
    } else {
        Err(SubscriptionError::Invalid(
            "remote manifest URL must be absolute HTTP(S) without credentials".into(),
        )
        .into())
    }
}

fn profile_mode(profile: &Profile) -> &str {
    profile
        .extra
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("local")
}

fn output_prepare(
    change: &CoreChange,
    render: &sempre_manager::SubscriptionRender,
    json_output: bool,
) -> Result<(), ClientError> {
    if json_output {
        print_json(&json!({ "change": change, "render": render }))
    } else {
        println!("{}.", change.message.trim_end_matches('.'));
        println!(
            "Artifact: {} ({} nodes)",
            render.artifact_hash, render.node_count
        );
        Ok(())
    }
}

fn output_changes(changes: &[CoreChange], json_output: bool) -> Result<(), ClientError> {
    if json_output {
        return print_json(&json!({ "changes": changes }));
    }
    for change in changes {
        println!("{}.", change.message.trim_end_matches('.'));
    }
    Ok(())
}

fn output_change(change: &CoreChange, json_output: bool) -> Result<(), ClientError> {
    if json_output {
        print_json(change)
    } else {
        println!("{}.", change.message.trim_end_matches('.'));
        if change.needs_restart {
            println!("Saved locally; run 'sempre subscription update' before restarting the core.");
        }
        Ok(())
    }
}

fn print_json(value: &impl Serialize) -> Result<(), ClientError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(ClientError::Json)?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_url_requires_an_uncredentialed_http_origin() {
        assert!(validate_remote_url("https://example.com/manifest").is_ok());
        assert!(validate_remote_url("https://user:secret@example.com/manifest").is_err());
        assert!(validate_remote_url("file:///tmp/manifest").is_err());
    }
}
