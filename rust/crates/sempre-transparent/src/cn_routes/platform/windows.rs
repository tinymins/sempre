use serde_json::{Value, json};

use super::{Endpoint, Route, invalid};
use crate::{TransparentError, command};

async fn run(runner: &dyn command::Runner, request: Value) -> Result<String, TransparentError> {
    let bytes = serde_json::to_vec(&request).map_err(|e| invalid(e.to_string()))?;
    let output = command::require_success(
        "powershell.exe",
        runner
            .run(
                "powershell.exe",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    include_str!("windows.ps1"),
                ],
                Some(&bytes),
            )
            .await?,
    )?;
    Ok(output.stdout.trim().into())
}

pub(super) async fn endpoint(
    runner: &dyn command::Runner,
    name: &str,
) -> Result<Endpoint, TransparentError> {
    serde_json::from_str(&run(runner, json!({"operation":"endpoint", "name":name})).await?)
        .map_err(|e| invalid(e.to_string()))
}

pub(super) async fn uplinks(
    runner: &dyn command::Runner,
) -> Result<Vec<Endpoint>, TransparentError> {
    serde_json::from_str(&run(runner, json!({"operation":"uplinks"})).await?)
        .map_err(|e| invalid(e.to_string()))
}

pub(super) async fn preflight(
    runner: &dyn command::Runner,
    routes: &[Route],
) -> Result<(), TransparentError> {
    run(runner, json!({"operation":"preflight", "routes":routes})).await?;
    Ok(())
}

pub(super) async fn change(
    runner: &dyn command::Runner,
    routes: &[Route],
    add: bool,
) -> Result<(), TransparentError> {
    run(
        runner,
        json!({"operation":"change", "routes":routes, "add":add}),
    )
    .await?;
    Ok(())
}

#[allow(clippy::unused_async)] // Policy rules are required only on Linux.
pub(super) async fn policy(_: &dyn command::Runner, _: bool) -> Result<(), TransparentError> {
    Ok(())
}
