use std::{io, path::PathBuf};

use sempre_state::{Layout, Mode};

use crate::ClientError;

pub async fn run(purge: bool, yes: bool) -> Result<(), ClientError> {
    if !yes && !confirm(purge)? {
        return Err(ClientError::Cancelled);
    }
    let layout = Layout::for_mode(Mode::System)?;
    let result = sempre_manager::uninstall_application(&layout, purge).await?;
    println!("{}", completion_message(result));
    Ok(())
}

fn completion_message(result: sempre_manager::ApplicationUninstall) -> &'static str {
    if result.installation_removal_scheduled {
        return if result.purged {
            "Service and data removed. Installation directory removal is pending until this process exits."
        } else {
            "Service removed; configuration, subscriptions, Web listener, and password retained. Installation directory removal is pending until this process exits."
        };
    }
    if result.purged {
        "Sempre and all data were removed."
    } else {
        "Sempre was removed. Configuration, subscriptions, Web listener, and password were retained."
    }
}

fn confirm(purge: bool) -> Result<bool, ClientError> {
    if purge {
        eprint!("Remove Sempre and all configuration, subscriptions, passwords, and data? [y/N]: ");
    } else {
        eprint!("Uninstall Sempre while retaining configuration and Web settings? [y/N]: ");
    }
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|source| ClientError::Io {
            operation: "read uninstall confirmation",
            path: PathBuf::from("stdin"),
            source,
        })?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_removal_never_claims_application_is_already_removed() {
        for purged in [false, true] {
            let message = completion_message(sempre_manager::ApplicationUninstall {
                purged,
                installation_removal_scheduled: true,
            });
            assert!(message.contains("pending"));
            assert!(!message.contains("Sempre was removed"));
            assert!(!message.contains("Sempre and all data were removed"));
        }
    }
}
