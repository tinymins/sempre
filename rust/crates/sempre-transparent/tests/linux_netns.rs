#![cfg(target_os = "linux")]

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    process::Command,
    time::Duration,
};

use sempre_transparent::{BYPASS_MARK, Controller, Mode, Plan};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::{net::TcpListener, time::timeout};

const PROXY_PORT: u16 = 17_893;
const DNS_PORT: u16 = 11_053;

#[tokio::test]
#[ignore = "requires root inside an isolated Linux network namespace"]
async fn tproxy_owns_intercepts_and_cleans_kernel_state() {
    if std::env::var("SEMPRE_NETNS_TEST").as_deref() != Ok("1") {
        return;
    }
    require_command("ip", &["-Version"]);
    require_command("nft", &["--version"]);
    command("ip", &["link", "add", "upstream", "type", "dummy"]);
    command("ip", &["address", "add", "192.0.2.1/24", "dev", "upstream"]);
    command("ip", &["link", "set", "upstream", "up"]);
    command("ip", &["route", "add", "default", "dev", "upstream"]);
    let root = tempfile::tempdir().expect("temporary layout");
    let layout = sempre_state::Layout::at(root.path());
    let controller = Controller::new(&layout);
    controller.cleanup().await.expect("clean initial state");

    command("nft", &["add", "table", "inet", "user_keep"]);
    command("nft", &["add", "table", "ip", "sempre_tproxy"]);
    assert!(controller.cleanup().await.is_err());
    assert!(tables().contains("table ip sempre_tproxy"));
    command("nft", &["delete", "table", "ip", "sempre_tproxy"]);

    let proxy = transparent_listener(PROXY_PORT);
    let dns = transparent_listener(DNS_PORT);
    let plan = Plan {
        core: "sing-box".into(),
        mode: Mode::TProxy,
        tproxy_port: PROXY_PORT,
        dns_port: DNS_PORT,
        capture_host: true,
        excluded_prefixes: vec!["127.0.0.0/8".into(), "::1/128".into(), "fc00::/7".into()],
        ..Plan::default()
    };
    controller.apply(&plan).await.expect("apply TProxy state");
    controller.verify(&plan).await.expect("verify TProxy state");
    assert!(tables().contains("table ip sempre_tproxy"));
    assert!(tables().contains("table ip6 sempre_tproxy"));
    drain_readiness(&proxy).await;
    drain_readiness(&dns).await;

    assert_tproxy_intercepted("203.0.113.10:443", &proxy).await;
    assert_redirected("8.8.8.8:53", &dns, DNS_PORT).await;

    controller.cleanup().await.expect("clean TProxy state");
    let tables = tables();
    assert!(!tables.contains("sempre_tproxy"));
    assert!(tables.contains("table inet user_keep"));
    assert!(!policy_state_exists("-4"));
    assert!(!policy_state_exists("-6"));
    command("nft", &["delete", "table", "inet", "user_keep"]);
}

async fn drain_readiness(listener: &TcpListener) {
    while timeout(Duration::from_millis(20), listener.accept())
        .await
        .is_ok()
    {}
}

fn transparent_listener(port: u16) -> TcpListener {
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).expect("TCP socket");
    socket
        .set_ip_transparent_v4(true)
        .expect("set IP_TRANSPARENT");
    socket.set_mark(BYPASS_MARK).expect("set bypass mark");
    socket.set_reuse_address(true).expect("set SO_REUSEADDR");
    socket
        .bind(&SockAddr::from(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            port,
        )))
        .expect("bind transparent listener");
    socket.listen(16).expect("listen");
    socket.set_nonblocking(true).expect("set nonblocking");
    let listener: std::net::TcpListener = socket.into();
    TcpListener::from_std(listener).expect("Tokio listener")
}

async fn assert_tproxy_intercepted(target: &str, listener: &TcpListener) {
    let (accepted, target) = intercepted_connection(target, listener).await;
    assert_eq!(accepted.local_addr().expect("original destination"), target);
}

async fn assert_redirected(target: &str, listener: &TcpListener, redirect_port: u16) {
    let (accepted, _) = intercepted_connection(target, listener).await;
    assert_eq!(
        accepted.local_addr().expect("redirect destination").port(),
        redirect_port
    );
}

async fn intercepted_connection(
    target: &str,
    listener: &TcpListener,
) -> (tokio::net::TcpStream, SocketAddr) {
    let target: SocketAddr = target.parse().expect("target address");
    let connect = tokio::net::TcpStream::connect(target);
    let accept = listener.accept();
    let (connected, accepted) = timeout(Duration::from_secs(3), async {
        tokio::join!(connect, accept)
    })
    .await
    .expect("transparent connection timeout");
    connected.expect("connect through TProxy");
    let (accepted, _) = accepted.expect("accept TProxy connection");
    (accepted, target)
}

fn policy_state_exists(family: &str) -> bool {
    let rules = output("ip", &["-j", family, "rule", "show"]);
    rules.contains("20240") || {
        let routes = output("ip", &["-j", family, "route", "show", "table", "20240"]);
        routes.contains("20240") || routes.contains("0.0.0.0/0") || routes.contains("::/0")
    }
}

fn tables() -> String {
    output("nft", &["list", "tables"])
}

fn require_command(program: &str, arguments: &[&str]) {
    let result = Command::new(program).args(arguments).output();
    assert!(result.is_ok(), "required command {program} is unavailable");
}

fn command(program: &str, arguments: &[&str]) {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run {program}: {error}"));
    assert!(
        result.status.success(),
        "{program} {} failed: {}{}",
        arguments.join(" "),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn output(program: &str, arguments: &[&str]) -> String {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run {program}: {error}"));
    format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
}
