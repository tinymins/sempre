use std::{fs, path::Path};

use sempre_converter::CustomNode;
use sempre_manager::Manager;
use sempre_state::{Layout, Mode, Store};
use serde_json::Value;

use crate::{ClientError, args::CustomNodeCommand, print_change};

pub(crate) fn run(mode: Mode, command: CustomNodeCommand, json: bool) -> Result<(), ClientError> {
    let manager = Manager::new(Store::new(Layout::for_mode(mode)?))?;
    match command {
        CustomNodeCommand::List => {
            let nodes = manager.custom_nodes()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&nodes)?);
            } else if nodes.is_empty() {
                println!("No custom nodes are configured.");
            } else {
                for node in nodes {
                    println!("{}\t{}", node.id, node.name);
                }
            }
        }
        CustomNodeCommand::Add { file } => {
            let mut node = read_candidate(&file)?;
            node.id.clear();
            println!(
                "{}",
                serde_json::to_string_pretty(&manager.save_custom_node(node)?)?
            );
        }
        CustomNodeCommand::Update { id, file } => {
            let mut node = read_candidate(&file)?;
            node.id = id;
            println!(
                "{}",
                serde_json::to_string_pretty(&manager.save_custom_node(node)?)?
            );
        }
        CustomNodeCommand::Remove { id } => {
            print_change(&manager.remove_custom_node(&id)?);
        }
    }
    Ok(())
}

fn read_candidate(path: &Path) -> Result<CustomNode, ClientError> {
    let data = fs::read(path).map_err(|source| ClientError::Io {
        operation: "read custom node",
        path: path.to_owned(),
        source,
    })?;
    if let Ok(node) = serde_json::from_slice::<CustomNode>(&data) {
        return Ok(node);
    }
    let proxy = serde_json::from_slice::<Value>(&data)?;
    let name = proxy
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok(CustomNode {
        id: String::new(),
        name,
        proxy,
        created_at: None,
        updated_at: None,
    })
}
