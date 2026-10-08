use std::{io, path::PathBuf};

use sempre_manager::{Manager, ManagerError};
use sempre_state::Layout;

use crate::{ClientError, VERSION};

pub(crate) async fn deploy_bundle(
    manager: &Manager,
    target: &Layout,
    kind: sempre_bundle::BundleKind,
    allow_replace: bool,
) -> Result<(), ClientError> {
    let result = match kind {
        sempre_bundle::BundleKind::Release => {
            manager
                .install_release(target, allow_replace, VERSION)
                .await
        }
        sempre_bundle::BundleKind::Snapshot => {
            manager.restore_bundle(target, allow_replace, VERSION).await
        }
    };
    let Err(ManagerError::ConfirmationRequired(message)) = result else {
        return result.map_err(ClientError::from);
    };
    if allow_replace || !confirm_replacement(&message)? {
        return Err(ClientError::Cancelled);
    }
    match kind {
        sempre_bundle::BundleKind::Release => {
            manager.install_release(target, true, VERSION).await?;
        }
        sempre_bundle::BundleKind::Snapshot => {
            manager.restore_bundle(target, true, VERSION).await?;
        }
    }
    Ok(())
}

fn confirm_replacement(message: &str) -> Result<bool, ClientError> {
    eprint!("{message}. Replace it? [y/N]: ");
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|source| ClientError::Io {
            operation: "read deployment confirmation",
            path: PathBuf::from("stdin"),
            source,
        })?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
