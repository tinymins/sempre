use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use serde_json::json;

use super::{
    CaptureController, CaptureIntent, CaptureSettings, Offload, Ruleset, discovery, host,
    parse_cidrs, script,
};
use crate::{TransparentError, command};

#[derive(Default)]
struct FakeRunner {
    calls: Mutex<Vec<String>>,
    fail_offload_check: bool,
}

impl command::Runner for FakeRunner {
    fn run<'a>(
        &'a self,
        program: &'a str,
        arguments: &'a [&'a str],
        input: Option<&'a [u8]>,
    ) -> Pin<Box<dyn Future<Output = Result<command::Output, TransparentError>> + Send + 'a>> {
        let line = format!("{program} {}", arguments.join(" "));
        let text = input.map(|data| String::from_utf8_lossy(data).into_owned());
        self.calls.lock().unwrap().push(line.clone());
        let success = !(self.fail_offload_check
            && line == "nft -c -f -"
            && text
                .as_deref()
                .is_some_and(|text| text.contains("flowtable")));
        let stdout = match line.as_str() {
            "ip -o -4 addr show scope global" => {
                "5: br-lan    inet 192.168.1.1/24 brd 192.168.1.255 scope global br-lan\n7: pppoe-wan    inet 1.2.3.4/32 scope global pppoe-wan\n"
            }
            "ip -o link show" => {
                "1: lo: <LOOPBACK>\n5: br-lan: <BROADCAST>\n6: wireguard: <POINTOPOINT>\n7: eth1@if2: <UP>\n"
            }
            "ip -4 rule show" => "0:\tfrom all lookup local\n",
            "tc qdisc show" => "qdisc noqueue 0: dev lo root\nqdisc fq_codel 0: dev eth0 root\n",
            "ubus call network.interface.wan status" => r#"{"l3_device":"pppoe-wan"}"#,
            "ubus call network.interface dump" => {
                r#"{"interface":[{"l3_device":"pppoe-wan","device":"eth1"}]}"#
            }
            _ => "",
        };
        Box::pin(async move {
            Ok(command::Output {
                success,
                stdout: stdout.into(),
                stderr: String::new(),
            })
        })
    }
}

fn sysfs(root: &std::path::Path) {
    let device = |name: &str, kind: &str| {
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(root.join(name).join("type"), kind).unwrap();
    };
    device("br-lan", "1");
    std::fs::create_dir_all(root.join("br-lan/bridge")).unwrap();
    std::fs::create_dir_all(root.join("br-lan/lower_eth0")).unwrap();
    device("eth0", "1");
    device("eth1", "1");
    device("pppoe-wan", "512");
}

struct Fixture {
    root: tempfile::TempDir,
    runner: Arc<FakeRunner>,
    controller: CaptureController,
}

fn fixture(fail_offload_check: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let mut layout = sempre_state::Layout::at(root.path());
    layout.openwrt = true;
    std::fs::create_dir_all(&layout.runtime).unwrap();
    let sys = root.path().join("sys");
    sysfs(&sys);
    let proc_net = root.path().join("net");
    std::fs::create_dir_all(&proc_net).unwrap();
    std::fs::write(
        proc_net.join("tcp6"),
        "  sl  local_address rem_address st\n 0: 00000000000000000000000000000000:1ED5 00000000000000000000000000000000:0000 0A\n",
    )
    .unwrap();
    let runner = Arc::new(FakeRunner {
        fail_offload_check,
        ..FakeRunner::default()
    });
    let controller = CaptureController::with_runner(&layout, runner.clone(), sys, proc_net);
    Fixture {
        root,
        runner,
        controller,
    }
}

fn intent() -> CaptureIntent {
    CaptureIntent {
        settings: CaptureSettings {
            front_dns_port: Some(5553),
            ..CaptureSettings::default()
        },
        tproxy_port: 7893,
        dns_port: 5553,
    }
}

#[test]
fn ports_come_from_the_configuration() {
    let config = json!({"inbounds": [
        {"tag": "tproxy-in", "type": "tproxy", "listen_port": 7893},
        {"tag": "dns-in", "type": "direct", "listen_port": 1053},
    ]});
    let intent = CaptureIntent::from_config(CaptureSettings::default(), &config).unwrap();
    assert_eq!((intent.tproxy_port, intent.dns_port), (7893, 1053));

    let front = CaptureSettings {
        front_dns_port: Some(5553),
        ..CaptureSettings::default()
    };
    assert_eq!(
        CaptureIntent::from_config(front, &config).unwrap().dns_port,
        5553
    );
}

#[test]
fn missing_tproxy_inbound_is_refused() {
    let config = json!({"inbounds": [{"tag": "dns-in", "listen_port": 1053}]});
    let error = CaptureIntent::from_config(CaptureSettings::default(), &config).unwrap_err();
    assert!(error.to_string().contains("tproxy-in"));
}

#[test]
fn script_matches_the_reference_rules() {
    let text = script(&Ruleset {
        tproxy_port: 7893,
        dns_port: 5553,
        lan_interfaces: vec!["br-lan".into()],
        local_nets: vec!["192.168.1.1/24".into()],
        gateways: vec!["192.168.1.1".into()],
        bypass_macs: vec!["aa:bb:cc:dd:ee:ff".into()],
        cn_prefixes: vec!["1.0.1.0/24".into()],
        offload: None,
    });
    assert!(text.starts_with("add table inet sempre_capture\ndelete table inet sempre_capture\n"));
    for expected in [
        "elements = { 1.0.1.0/24 };",
        "set bypass_macs { type ether_addr; elements = { aa:bb:cc:dd:ee:ff }; }",
        "set lan_devices { type ifname; elements = { \"br-lan\" }; }",
        "udp dport 53 counter return",
        "ether saddr @bypass_macs counter return",
        "iifname @lan_devices meta l4proto { tcp, udp } counter tproxy ip to :7893 meta mark set 0x2333 accept",
        "ip daddr @gateways meta l4proto { tcp, udp } th dport 53 counter redirect to :5553",
        "meta nfproto ipv6 iifname @lan_devices meta l4proto { tcp, udp } th dport 53 counter redirect to :5553",
    ] {
        assert!(text.contains(expected), "missing {expected}\n{text}");
    }
    assert!(!text.contains("flowtable"));
}

#[test]
fn offload_rules_only_accelerate_direct_traffic() {
    let text = script(&Ruleset {
        offload: Some(Offload {
            devices: vec!["eth0".into(), "eth1".into()],
            lan: vec!["br-lan".into()],
            wan: vec!["pppoe-wan".into()],
        }),
        ..Ruleset::default()
    });
    assert!(text.contains(
        "flowtable direct { hook ingress priority 0; devices = { \"eth0\", \"eth1\" }; counter; }"
    ));
    assert!(text.contains("ip daddr @cn_ip counter flow add @direct"));
    assert!(text.contains("meta mark != 0 return"));
}

#[test]
fn empty_sets_stay_valid() {
    let text = script(&Ruleset::default());
    assert!(text.contains("set cn_ip { type ipv4_addr; flags interval; auto-merge; }"));
    assert!(text.contains("set bypass_macs { type ether_addr; }"));
}

#[test]
fn host_output_parsers() {
    assert_eq!(
        discovery::bridge_addresses(
            "5: br-lan    inet 192.168.1.1/24 brd x\n7: eth1    inet 1.2.3.4/32\n"
        ),
        vec![("br-lan".to_owned(), "192.168.1.1/24".to_owned())]
    );
    assert_eq!(
        discovery::lan_interfaces(
            "1: lo: <L>\n5: br-lan: <B>\n6: wireguard: <P>\n8: br-x@eth0: <U>\n"
        ),
        vec!["br-lan", "wireguard", "br-x"]
    );
    assert!(host::table_has_listener(
        "h\n 0: 0100007F:1ED5 00000000:0000 0A\n",
        7893
    ));
    assert!(!host::table_has_listener(
        "h\n 0: 0100007F:1ED5 01010101:0050 01\n",
        7893
    ));
    assert!(discovery::traffic_control_active(
        "qdisc cake 8001: dev eth1 root\n"
    ));
    assert!(!discovery::traffic_control_active(
        "qdisc fq_codel 0: dev eth1 root\n"
    ));
}

#[test]
fn geoip_rejects_a_list_with_any_invalid_line() {
    assert_eq!(
        parse_cidrs("# c\n1.0.1.0/24\n\n8.8.8.8\n").unwrap().len(),
        2
    );
    assert!(parse_cidrs("1.0.1.0/24\n<html>\n").is_err());
    assert!(parse_cidrs("1.0.1.0/33\n").is_err());
}

#[tokio::test]
async fn apply_is_atomic_and_routes_before_loading() {
    let fixture = fixture(false);
    fixture.controller.apply(&intent()).await.unwrap();
    let calls = fixture.runner.calls.lock().unwrap().clone();
    let position = |needle: &str| calls.iter().position(|call| call == needle).unwrap();
    assert!(position("nft -c -f -") < position("nft -f -"));
    assert!(
        position("ip -4 rule add priority 10010 fwmark 0x2333 table 100") < position("nft -f -")
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.as_str() == "nft -f -")
            .count(),
        1
    );
    let status = fixture.controller.status();
    assert_eq!(status["offload"], true);
    assert_eq!(status["offload_reason"], "selective-ipv4-tcp");
    assert!(
        calls
            .iter()
            .any(|call| call.starts_with("uci set firewall.sempre_capture.path="))
    );
}

#[tokio::test]
async fn kernel_without_flowtable_falls_back() {
    let fixture = fixture(true);
    fixture.controller.apply(&intent()).await.unwrap();
    let status = fixture.controller.status();
    assert_eq!(status["offload"], false);
    assert_eq!(status["offload_reason"], "kernel-unavailable");
}

#[tokio::test]
async fn withdraw_stops_later_reapplies() {
    let fixture = fixture(false);
    fixture.controller.apply(&intent()).await.unwrap();
    assert!(fixture.controller.reapply().await.unwrap());
    fixture.controller.withdraw().await.unwrap();
    assert!(!fixture.controller.reapply().await.unwrap());
    assert_eq!(fixture.controller.status()["active"], false);
}

#[tokio::test]
async fn waiting_for_a_slow_core_lasts_until_it_listens() {
    let fixture = fixture(false);
    let table = fixture.root.path().join("net/tcp6");
    let listening = std::fs::read_to_string(&table).unwrap();
    std::fs::write(&table, "  sl  local_address rem_address st\n").unwrap();
    let waiting = fixture.controller.wait_until_listening(7893);
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(1200), &mut waiting)
            .await
            .is_err()
    );
    std::fs::write(&table, listening).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), waiting)
        .await
        .unwrap();
}

#[tokio::test]
async fn unregister_removes_rules_and_the_firewall_include() {
    let fixture = fixture(false);
    fixture.controller.apply(&intent()).await.unwrap();
    let include = sempre_state::Layout::at(fixture.root.path())
        .home
        .join("firewall.include");
    assert!(include.is_file());
    fixture.controller.unregister().await.unwrap();
    assert!(!include.exists());
    let calls = fixture.runner.calls.lock().unwrap().clone();
    assert!(
        calls
            .iter()
            .any(|call| call == "uci delete firewall.sempre_capture")
    );
    assert_eq!(
        calls.last().map(String::as_str),
        Some("uci commit firewall")
    );
    assert_eq!(fixture.controller.status()["active"], false);
}
