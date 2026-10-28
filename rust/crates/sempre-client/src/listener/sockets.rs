use std::{io, net::SocketAddr};

use sempre_control::validate_listens;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::{TcpListener, TcpStream};

use crate::ClientError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Spec {
    address: SocketAddr,
    v6_only: bool,
}

struct Bound {
    spec: Spec,
    listener: TcpListener,
}

pub(crate) struct Sockets(Vec<Bound>);

pub(super) struct ChangeError {
    pub message: String,
    pub fatal: bool,
}

impl Sockets {
    pub async fn bind(values: &[String]) -> Result<Self, ClientError> {
        let mut sockets = Vec::new();
        let (specs, mut prepared) = resolve(values, &[]).await?;
        sockets.append(&mut prepared);
        for spec in specs {
            if sockets.iter().any(|bound| bound.spec == spec) {
                continue;
            }
            sockets.push(bind(spec)?);
        }
        Ok(Self(sockets))
    }

    pub fn addresses(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|bound| bound.spec.address.to_string())
            .collect()
    }

    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let accepted = futures_util::future::select_all(
            self.0.iter().map(|bound| Box::pin(bound.listener.accept())),
        )
        .await;
        accepted.0
    }

    pub(super) async fn change<T>(
        &mut self,
        values: &[String],
        commit: impl FnOnce(&[String]) -> Result<T, ClientError>,
    ) -> Result<T, ChangeError> {
        let (specs, prepared) = resolve(values, &self.0)
            .await
            .map_err(|error| ChangeError {
                message: error.to_string(),
                fatal: false,
            })?;
        let original = self.0.iter().map(|bound| bound.spec).collect::<Vec<_>>();
        let mut retired = Vec::new();
        let mut next = Vec::new();
        for bound in std::mem::take(&mut self.0) {
            if specs.contains(&bound.spec) {
                next.push(bound);
            } else {
                retired.push(bound);
            }
        }
        next.extend(prepared);
        // Release only sockets that prevent binding the replacement. Accepted HTTP connections remain open.
        retired.retain(|bound| !specs.iter().any(|spec| overlaps(bound.spec, *spec)));
        let prepared = (|| {
            for spec in &specs {
                if !next.iter().any(|bound| bound.spec == *spec) {
                    next.push(bind(*spec)?);
                }
            }
            let addresses = specs
                .iter()
                .map(|spec| spec.address.to_string())
                .collect::<Vec<_>>();
            commit(&addresses)
        })();
        match prepared {
            Ok(value) => {
                next.sort_by_key(|bound| {
                    specs.iter().position(|spec| *spec == bound.spec).unwrap()
                });
                self.0 = next;
                Ok(value)
            }
            Err(error) => {
                next.retain(|bound| original.contains(&bound.spec));
                next.extend(retired);
                for spec in &original {
                    if !next.iter().any(|bound| bound.spec == *spec) {
                        match bind(*spec) {
                            Ok(bound) => next.push(bound),
                            Err(restore) => {
                                return Err(ChangeError {
                                    message: format!(
                                        "{error}; restoring previous listener failed: {restore}; web listener is shutting down"
                                    ),
                                    fatal: true,
                                });
                            }
                        }
                    }
                }
                next.sort_by_key(|bound| {
                    original
                        .iter()
                        .position(|spec| *spec == bound.spec)
                        .unwrap()
                });
                self.0 = next;
                Err(ChangeError {
                    message: error.to_string(),
                    fatal: false,
                })
            }
        }
    }
}

async fn resolve(
    values: &[String],
    current: &[Bound],
) -> Result<(Vec<Spec>, Vec<Bound>), ClientError> {
    validate_listens(values)?;
    if values.len() == 1 && values[0].parse::<SocketAddr>().is_err() {
        let candidates = tokio::net::lookup_host(&values[0])
            .await
            .map_err(|source| ClientError::Bind {
                address: values[0].clone(),
                source,
            })?;
        let mut last_error =
            ClientError::Runtime(format!("listen address {} did not resolve", values[0]));
        for address in candidates {
            let spec = Spec {
                address,
                v6_only: false,
            };
            if current.iter().any(|bound| overlaps(bound.spec, spec)) {
                return Ok((vec![spec], Vec::new()));
            }
            match bind(spec) {
                Ok(bound) => return Ok((vec![spec], vec![bound])),
                Err(error) => last_error = error,
            }
        }
        return Err(last_error);
    }
    let addresses = values
        .iter()
        .map(|value| value.parse::<SocketAddr>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ClientError::Runtime(error.to_string()))?;
    let has_ipv4 = addresses.iter().any(SocketAddr::is_ipv4);
    Ok((
        addresses
            .into_iter()
            .map(|address| Spec {
                address,
                v6_only: address.is_ipv6() && has_ipv4,
            })
            .collect(),
        Vec::new(),
    ))
}

fn bind(spec: Spec) -> Result<Bound, ClientError> {
    let listener = (|| -> io::Result<TcpListener> {
        let socket = Socket::new(
            Domain::for_address(spec.address),
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        #[cfg(unix)]
        socket.set_reuse_address(true)?;
        // Keep the platform's existing single-IPv6 behavior; explicit IPv4 listeners need separate sockets.
        if spec.v6_only {
            socket.set_only_v6(true)?;
        }
        socket.set_nonblocking(true)?;
        socket.bind(&spec.address.into())?;
        socket.listen(1024)?;
        TcpListener::from_std(socket.into())
    })()
    .map_err(|source| ClientError::Bind {
        address: spec.address.to_string(),
        source,
    })?;
    Ok(Bound { spec, listener })
}

fn overlaps(left: Spec, right: Spec) -> bool {
    left.address.port() == right.address.port()
        && (left.address == right.address
            || (left.address.is_ipv4() == right.address.is_ipv4()
                && (left.address.ip().is_unspecified() || right.address.ip().is_unspecified()))
            || (left.address.is_ipv6()
                && left.address.ip().is_unspecified()
                && !left.v6_only
                && right.address.is_ipv4())
            || (right.address.is_ipv6()
                && right.address.ip().is_unspecified()
                && !right.v6_only
                && left.address.is_ipv4()))
}
