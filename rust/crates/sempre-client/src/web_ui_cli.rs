use std::io::{self, Read as _};

use sempre_control::{WebConfigStore, local_url};
use sempre_state::{Layout, Mode, Store};
use serde::{Deserialize, Serialize};

use crate::{
    ClientError,
    args::{UiCommand, WebCommand, WebPasswordCommand},
    local_api::LocalApi,
    ui_distribution,
};

#[derive(Deserialize, Serialize)]
struct WebStatus {
    listen: String,
    local_url: String,
    password_set: bool,
}

#[derive(Serialize)]
struct WebPatch<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    listen: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
}

pub(crate) async fn run_web(
    mode: Mode,
    command: WebCommand,
    json: bool,
) -> Result<(), ClientError> {
    let layout = initialized_layout(mode)?;
    let web = WebConfigStore::new(&layout.web_config);
    let status = match command {
        WebCommand::Status => offline_status(&web)?,
        WebCommand::Listen { address } => {
            if let Ok(client) = LocalApi::discover(&layout.daemon_control) {
                client
                    .patch(
                        "/api/v1/web",
                        &WebPatch {
                            listen: Some(&address),
                            password: None,
                        },
                    )
                    .await?
            } else {
                web.set_listen(&address)?;
                println!("Web listener saved; it will apply when the daemon starts.");
                offline_status(&web)?
            }
        }
        WebCommand::Password { command } => {
            let password = match command {
                WebPasswordCommand::Set { stdin: true } => read_password()?,
                WebPasswordCommand::Clear => String::new(),
                WebPasswordCommand::Set { stdin: false } => unreachable!("clap requires --stdin"),
            };
            if let Ok(client) = LocalApi::discover(&layout.daemon_control) {
                client
                    .patch(
                        "/api/v1/web",
                        &WebPatch {
                            listen: None,
                            password: Some(&password),
                        },
                    )
                    .await?
            } else {
                web.set_password(&password)?;
                offline_status(&web)?
            }
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("Listen: {}", status.listen);
        println!("Local URL: {}", status.local_url);
        println!("Password set: {}", status.password_set);
    }
    Ok(())
}

pub(crate) async fn run_ui(mode: Mode, command: UiCommand, json: bool) -> Result<(), ClientError> {
    let layout = initialized_layout(mode)?;
    let store = sempre_ui::Store::new(&layout.ui);
    match command {
        UiCommand::Status => match store.current() {
            Ok(metadata) => print_metadata(&metadata, json)?,
            Err(sempre_ui::UiError::Read(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                if json {
                    println!("{{\n  \"installed\": false\n}}");
                } else {
                    println!("UI: not installed");
                }
            }
            Err(error) => return Err(error.into()),
        },
        UiCommand::Install { source, sha256 } => {
            let metadata =
                ui_distribution::install(&layout, &source, sha256.as_deref().unwrap_or_default())
                    .await
                    .map_err(|error| ClientError::Runtime(format!("install UI: {error}")))?;
            print_metadata(&metadata, json)?;
        }
        UiCommand::Update => {
            let metadata = ui_distribution::update(&layout)
                .await
                .map_err(|error| ClientError::Runtime(format!("update UI: {error}")))?;
            print_metadata(&metadata, json)?;
        }
        UiCommand::Remove => {
            store.remove()?;
            if json {
                println!("{{\n  \"installed\": false\n}}");
            } else {
                println!("UI removed.");
            }
        }
    }
    Ok(())
}

fn initialized_layout(mode: Mode) -> Result<Layout, ClientError> {
    let layout = Layout::for_mode(mode)?;
    Store::new(layout.clone()).initialize()?;
    WebConfigStore::new(&layout.web_config).initialize()?;
    Ok(layout)
}

fn offline_status(store: &WebConfigStore) -> Result<WebStatus, ClientError> {
    let config = store.read()?;
    let password_set = config.password_protected();
    Ok(WebStatus {
        local_url: local_url(&config.listen)?,
        listen: config.listen,
        password_set,
    })
}

fn read_password() -> Result<String, ClientError> {
    let mut data = String::new();
    io::stdin()
        .take(1026)
        .read_to_string(&mut data)
        .map_err(|source| ClientError::Io {
            operation: "read administrator password",
            path: "stdin".into(),
            source,
        })?;
    let password = data.trim_end_matches(['\r', '\n']).to_owned();
    if password.is_empty() {
        return Err(ClientError::Runtime(
            "password from stdin is empty; use 'web password clear' explicitly".into(),
        ));
    }
    if password.len() > 1024 {
        return Err(ClientError::Runtime("password is too long".into()));
    }
    Ok(password)
}

fn print_metadata(metadata: &sempre_ui::Metadata, json: bool) -> Result<(), ClientError> {
    if json {
        println!("{}", serde_json::to_string_pretty(metadata)?);
    } else {
        println!(
            "UI: {} {} ({})",
            metadata.manifest.name, metadata.manifest.version, metadata.source_type
        );
    }
    Ok(())
}
