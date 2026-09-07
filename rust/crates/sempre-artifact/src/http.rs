use std::{
    net::{IpAddr, Ipv4Addr, UdpSocket},
    time::Duration,
};

use reqwest::{ClientBuilder, Method};

pub(crate) fn client_builder(user_agent: &str, timeout: Duration) -> ClientBuilder {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(crate::https_redirect_policy())
        .user_agent(user_agent);
    if !ipv6_egress_available() {
        builder = builder.local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }
    builder
}

pub(crate) fn github_retry() -> reqwest::retry::Builder {
    reqwest::retry::for_host("api.github.com")
        .classify_fn(|attempt| {
            if attempt.method() == Method::GET
                && (attempt.error().is_some()
                    || attempt
                        .status()
                        .is_some_and(|status| status.is_server_error() || status.as_u16() == 429))
            {
                attempt.retryable()
            } else {
                attempt.success()
            }
        })
        .max_retries_per_request(2)
        .no_budget()
}

fn ipv6_egress_available() -> bool {
    UdpSocket::bind("[::]:0")
        .and_then(|socket| socket.connect("[2606:4700:4700::1111]:53"))
        .is_ok()
}
