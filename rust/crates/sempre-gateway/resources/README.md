# Domestic domain snapshot

`domains-min.txt` is a release-bundled snapshot of the OhMyWrt domain list used by the trusted gateway reference implementation.

- Source: <https://github.com/ohmywrt/ohmywrt/blob/master/package/base-files/files/etc/domains-min.txt>
- SHA-256: `80aed7f0cbe1d0292f58284f5b0b91043e09950a9019c60da96bff3a6e8ba634`
- Domain count: `77072`

Sempre parses the original AdGuard upstream-file shape and treats the bundled snapshot as its startup-safe baseline. Updating the snapshot requires updating the checksum and count assertions in the same change.
