// Offline routing regression against real sing-box binaries. No TUN, system DNS,
// privileged ports, public upstreams, installed service, or user profile is used.
// cargo build --manifest-path=rust/Cargo.toml -p sempre-converter-cli
// bun rust/scripts/dns_route_smoke.ts /path/to/sing-box [more/core/binaries...]
import assert from "node:assert/strict";
import { createSocket } from "node:dgram";
import { createServer as httpServer } from "node:http";
import { mkdtemp } from "node:fs/promises";
import { createServer, connect } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve("rust/target/debug/sempre-converter");
const binaries = process.argv.slice(2).map((value) => resolve(value));
assert(binaries.length, "Pass at least one sing-box binary");
const evidence = await mkdtemp(join(tmpdir(), "sempre-dns-route-smoke-"));

async function command(args: string[], input?: string) {
  const child = Bun.spawn(args, { stdin: input === undefined ? "ignore" : new Blob([input]), stdout: "pipe", stderr: "pipe" });
  const [code, stdout, stderr] = await Promise.all([
    child.exited, new Response(child.stdout).text(), new Response(child.stderr).text(),
  ]);
  assert.equal(code, 0, `${args[0]}: ${stderr || stdout}`);
  return stdout;
}

async function freePort() {
  const server = createServer();
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const port = (server.address() as { port: number }).port;
  await new Promise<void>((done) => server.close(() => done()));
  return port;
}

function questionEnd(message: Buffer) {
  let cursor = 12;
  while (message[cursor]) cursor += message[cursor] + 1;
  return cursor + 5;
}

async function resolver(remote: boolean) {
  const queries: string[] = [];
  const socket = createSocket("udp4");
  socket.on("message", (message, peer) => {
    const labels: string[] = [];
    for (let cursor = 12; message[cursor];) {
      const length = message[cursor++];
      labels.push(message.toString("ascii", cursor, cursor + length));
      cursor += length;
    }
    const name = labels.join(".");
    queries.push(name);
    const end = questionEnd(message);
    const isA = message.readUInt16BE(end - 4) === 1;
    const header = Buffer.from(message.subarray(0, 12));
    header.writeUInt16BE(0x8180, 2);
    header.writeUInt16BE(isA ? 1 : 0, 6);
    header.writeUInt32BE(0, 8);
    // A local answer for an unknown name deliberately points at the wrong host.
    const address = name === "known.test" ? [203, 0, 113, 1]
      : name === "geoip.test" && remote ? [203, 0, 113, 2]
      : remote ? [198, 51, 100, 2] : [203, 0, 113, 66];
    const answer = Buffer.from([0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, ...address]);
    socket.send(Buffer.concat([header, message.subarray(12, end), ...(isA ? [answer] : [])]), peer.port, peer.address);
  });
  await new Promise<void>((done) => socket.bind(0, "127.0.0.1", done));
  return { port: socket.address().port, queries, close: () => socket.close() };
}

async function dnsQuery(port: number, name: string) {
  const socket = createSocket("udp4");
  const header = Buffer.from([0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
  const labels = name.split(".").flatMap((label) => [label.length, ...Buffer.from(label)]);
  try {
    return await new Promise<string>((done, reject) => {
      const timer = setTimeout(() => reject(new Error(`DNS timeout: ${name}`)), 5000);
      socket.once("message", (message) => {
        clearTimeout(timer);
        const end = questionEnd(message);
        if (message.readUInt16BE(6) !== 1) return reject(new Error("Expected one A answer"));
        let answer = end;
        while (message[answer] && (message[answer] & 0xc0) !== 0xc0) answer += message[answer] + 1;
        answer += (message[answer] & 0xc0) === 0xc0 ? 2 : 1;
        done([...message.subarray(answer + 10, answer + 14)].join("."));
      });
      socket.send(Buffer.concat([header, Buffer.from([...labels, 0, 0, 1, 0, 1])]), port, "127.0.0.1");
    });
  } finally { socket.close(); }
}

async function proxy() {
  const destinations: string[] = [];
  const server = httpServer();
  server.on("connect", (request, socket, head) => {
    destinations.push(request.url!);
    socket.on("error", () => {});
    socket.write("HTTP/1.1 200 Connection Established\r\n\r\n");
    const reply = () => socket.end("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK");
    if (head.length) reply(); else socket.once("data", reply);
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  return {
    port: (server.address() as { port: number }).port, destinations,
    close: () => server.close(),
  };
}

async function run(binary: string, fakeip: boolean) {
  const version = await command([binary, "version"]);
  const minor = version.match(/sing-box version 1\.(12|13|14)\./)?.[1];
  assert(minor, "Smoke test supports sing-box 1.12, 1.13, 1.14");
  const id = `v${minor}-${fakeip ? "fake" : "real"}`;
  const root = await mkdtemp(join(evidence, `${id}-`));
  const local = await resolver(false);
  const remote = await resolver(true);
  const direct = await proxy();
  const foreign = await proxy();
  let core: ReturnType<typeof Bun.spawn> | undefined;
  let output: Promise<string> | undefined;
  try {
    const socks = await freePort();
    const dns = await freePort();
    const prime = await freePort();
    const request = {
      protocol: 1, target: { format: `sing-box-v${minor}` }, snapshots: [], custom_nodes: [],
      profile: {
        groups: [{ name: "foreign", type: "select" }],
        dns: { shared: { fakeipEnabled: fakeip, managedDnsFrontend: true, systemDnsTakeoverEnabled: true } },
      },
    };
    const compiled = JSON.parse(await command([compiler], JSON.stringify(request)));
    const config = JSON.parse(compiled.content);
    // Keep the generated route policy and cache semantics. Replace external I/O
    // with loopback fixtures; remote TLS/detour are checked in converter tests.
    config.log = { level: "debug" };
    delete config.experimental;
    config.inbounds = [
      { type: "mixed", tag: "probe", listen: "127.0.0.1", listen_port: socks },
      { type: "direct", tag: "sempre-dns-core-in", listen: "127.0.0.1", listen_port: dns },
      { type: "direct", tag: "cache-prime", listen: "127.0.0.1", listen_port: prime },
    ];
    config.outbounds = [
      { type: "http", tag: "direct", server: "127.0.0.1", server_port: direct.port },
      { type: "http", tag: "foreign", server: "127.0.0.1", server_port: foreign.port },
    ];
    config.route.rules.unshift({ inbound: "cache-prime", action: "hijack-dns" });
    config.route.rule_set = [
      { type: "inline", tag: "geosite-cn", rules: [{ domain: ["known.test"] }] },
      { type: "inline", tag: "geoip-cn", rules: [{ ip_cidr: ["203.0.113.0/24"] }] },
      { type: "inline", tag: "geoip-hk", rules: [{ ip_cidr: ["192.0.2.0/24"] }] },
    ];
    config.dns.strategy = "ipv4_only";
    config.dns.rules.unshift({ inbound: "cache-prime", action: "route", server: "local" });
    config.dns.servers = config.dns.servers.map((server: { tag: string }) => server.tag === "fakeip" ? server : {
      type: "udp", tag: server.tag, server: "127.0.0.1",
      server_port: server.tag === "remote" ? remote.port : local.port,
    });
    const file = join(root, "config.json");
    await Bun.write(file, JSON.stringify(config, null, 2));
    await command([binary, "check", "-c", file]);
    core = Bun.spawn([binary, "run", "-c", file, "-D", root, "--disable-color"], { stdout: "pipe", stderr: "pipe" });
    output = Promise.all([new Response(core.stdout).text(), new Response(core.stderr).text()]).then((logs) => logs.join("\n"));
    let ready = false;
    for (let attempt = 0; attempt < 100 && !ready; attempt++) {
      ready = await new Promise<boolean>((done) => {
        const socket = connect(socks, "127.0.0.1");
        socket.on("connect", () => { socket.destroy(); done(true); });
        socket.on("error", () => done(false));
      });
      if (!ready) await Bun.sleep(20);
    }
    assert(ready, "Core did not start");
    for (const name of ["foreign.test", "geoip.test", "known.test", "cached.test"]) {
      const address = fakeip ? await dnsQuery(dns, name) : undefined;
      if (fakeip) assert.match(address!, /^198\.18\./);
      if (name === "cached.test") assert.equal(await dnsQuery(prime, name), "203.0.113.66");
      const remoteBefore = remote.queries.length;
      const result = await command([
        "curl", "--noproxy", "", "--max-time", "5", "-fsS",
        ...(fakeip ? ["--socks5", `127.0.0.1:${socks}`, "--resolve", `${name}:80:${address}`]
          : ["--socks5-hostname", `127.0.0.1:${socks}`]),
        `http://${name}/`,
      ]);
      assert.equal(result, "OK");
      if (name === "known.test") {
        assert.equal(remote.queries.length, remoteBefore, "Geosite must not query remote DNS");
        assert.equal(direct.destinations.at(-1), "known.test:80");
      } else if (name === "geoip.test") {
        assert.equal(direct.destinations.at(-1), "203.0.113.2:80");
        assert(remote.queries.includes(name));
      } else {
        assert.equal(foreign.destinations.at(-1), "198.51.100.2:80", "Do not dial a poisoned local answer");
        assert(remote.queries.includes(name));
      }
      assert(!direct.destinations.includes("203.0.113.66:80"), "Poisoned destination used");
      console.log(`PASS ${id} ${name}`);
    }
  } finally {
    if (core) { core.kill("SIGTERM"); await core.exited; }
    if (output) await Bun.write(join(root, "core.log"), await output);
    local.close(); remote.close(); direct.close(); foreign.close();
  }
}

console.log(`Evidence: ${evidence}`);
for (const binary of binaries) {
  for (const fakeip of [true, false]) await run(binary, fakeip);
}
