use tokio::net::TcpStream;

use crate::{Plan, TransparentError, command};

pub(super) async fn listeners_ready(
    runner: &dyn command::Runner,
    plan: &Plan,
) -> Result<(), TransparentError> {
    tproxy_listener_ready(runner, plan.tproxy_port).await?;
    TcpStream::connect(("127.0.0.1", plan.dns_port))
        .await
        .map_err(|error| {
            TransparentError::Invalid(format!(
                "TCP port {} is not listening: {error}",
                plan.dns_port
            ))
        })?;
    Ok(())
}

async fn tproxy_listener_ready(
    runner: &dyn command::Runner,
    port: u16,
) -> Result<(), TransparentError> {
    let filter = format!("sport = :{port}");
    let output = command::require_success(
        "ss",
        runner.run("ss", &["-H", "-ltn", &filter], None).await?,
    )?;
    if has_listener(&output.stdout) {
        Ok(())
    } else {
        Err(TransparentError::Invalid(format!(
            "TCP port {port} is not listening"
        )))
    }
}

fn has_listener(output: &str) -> bool {
    output.lines().any(|line| !line.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_non_empty_listener_rows() {
        assert!(!has_listener(""));
        assert!(!has_listener("\n  \n"));
        assert!(has_listener("LISTEN 0 4096 *:20582 *:*\n"));
    }
}
