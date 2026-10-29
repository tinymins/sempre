use std::os::fd::{AsRawFd, OwnedFd};

use nix::{
    errno::Errno,
    sys::socket::{self, AddressFamily, MsgFlags, SockFlag, SockType},
};
use tokio::io::unix::AsyncFd;

use crate::TransparentError;

pub(super) struct Watcher {
    socket: AsyncFd<OwnedFd>,
    buffer: Vec<u8>,
}

impl Watcher {
    pub(super) fn open() -> Result<Self, TransparentError> {
        let socket = subscribe().map_err(|e| failure("open route event socket", e))?;
        Ok(Self {
            socket: AsyncFd::new(socket).map_err(|source| TransparentError::Io {
                context: "watch route event socket".into(),
                source,
            })?,
            buffer: vec![0; 64 * 1024],
        })
    }

    pub(super) async fn next(&mut self) -> Result<(), TransparentError> {
        loop {
            let mut guard =
                self.socket
                    .readable()
                    .await
                    .map_err(|source| TransparentError::Io {
                        context: "wait for route events".into(),
                        source,
                    })?;
            let mut changed = false;
            loop {
                // Reads never block, so the descriptor keeps its default mode.
                match socket::recv(
                    self.socket.get_ref().as_raw_fd(),
                    &mut self.buffer,
                    MsgFlags::MSG_DONTWAIT,
                ) {
                    Ok(length) => changed |= relevant(&self.buffer[..length]),
                    Err(Errno::EAGAIN) => {
                        guard.clear_ready();
                        break;
                    }
                    // The kernel dropped notifications, so something changed.
                    Err(Errno::ENOBUFS) => changed = true,
                    Err(Errno::EINTR) => {}
                    Err(error) => return Err(failure("read route events", error)),
                }
            }
            if changed {
                return Ok(());
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn subscribe() -> Result<OwnedFd, Errno> {
    let socket = socket::socket(
        AddressFamily::Netlink,
        SockType::Raw,
        SockFlag::SOCK_CLOEXEC,
        socket::SockProtocol::NetlinkRoute,
    )?;
    let groups = nix::libc::RTMGRP_LINK
        | nix::libc::RTMGRP_IPV4_IFADDR
        | nix::libc::RTMGRP_IPV4_ROUTE
        | nix::libc::RTMGRP_IPV6_IFADDR
        | nix::libc::RTMGRP_IPV6_ROUTE;
    socket::bind(
        socket.as_raw_fd(),
        &socket::NetlinkAddr::new(0, groups.cast_unsigned()),
    )?;
    Ok(socket)
}

#[cfg(target_os = "linux")]
fn relevant(message: &[u8]) -> bool {
    // Only the subscribed multicast groups are delivered.
    !message.is_empty()
}

#[cfg(target_os = "macos")]
fn subscribe() -> Result<OwnedFd, Errno> {
    socket::socket(AddressFamily::Route, SockType::Raw, SockFlag::empty(), None)
}

#[cfg(target_os = "macos")]
fn relevant(message: &[u8]) -> bool {
    // Every route socket also receives RTM_GET replies, including those of the
    // route audit itself; reacting to them would loop.
    message.get(3).is_some_and(|kind| {
        [
            nix::libc::RTM_ADD,
            nix::libc::RTM_DELETE,
            nix::libc::RTM_CHANGE,
            nix::libc::RTM_NEWADDR,
            nix::libc::RTM_DELADDR,
            nix::libc::RTM_IFINFO,
        ]
        .contains(&i32::from(*kind))
    })
}

fn failure(context: &str, error: Errno) -> TransparentError {
    TransparentError::Io {
        context: context.into(),
        source: error.into(),
    }
}
