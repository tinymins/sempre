use std::{io, path::PathBuf};

use sempre_state::{Layout, Mode};

use crate::ClientError;

pub async fn run(purge: bool, yes: bool) -> Result<(), ClientError> {
    if !yes && !confirm(purge)? {
        return Err(ClientError::Cancelled);
    }
    let layout = Layout::for_mode(Mode::System)?;
    let result = sempre_manager::uninstall_application(&layout, purge).await?;
    if result.purged {
        println!("Sempre and all data were removed.");
    } else {
        println!(
            "Sempre was removed. Configuration, subscriptions, Web listener, and password were retained."
        );
    }
    if result.installation_removal_scheduled {
        println!("The installation directory will be removed after this process exits.");
    }
    Ok(())
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
