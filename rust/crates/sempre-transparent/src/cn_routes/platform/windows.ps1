[Console]::InputEncoding = [Text.UTF8Encoding]::new()
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
$ErrorActionPreference = 'Stop'
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
if ($request.operation -eq 'endpoint') {
    $item = Get-NetIPInterface | Where-Object InterfaceAlias -eq $request.name | Select-Object -First 1
    if (-not $item) { throw 'TUN interface is unavailable' }
    @{ interface = [string]$item.InterfaceAlias; index = [uint32]$item.InterfaceIndex; gateway = '' } | ConvertTo-Json -Compress
    exit
}
if ($request.operation -eq 'uplinks') {
    $items = foreach ($prefix in @('0.0.0.0/0', '::/0')) {
        $route = Get-NetRoute -PolicyStore ActiveStore -DestinationPrefix $prefix -ErrorAction SilentlyContinue |
            Sort-Object { $_.RouteMetric + $_.InterfaceMetric } | Select-Object -First 1
        if ($route) { @{ interface = [string]$route.InterfaceAlias; index = [uint32]$route.InterfaceIndex; gateway = [string]$route.NextHop } }
    }
    ConvertTo-Json -InputObject @($items) -Compress
    exit
}
if ($request.operation -eq 'preflight') {
    $prefixes = @{}
    foreach ($route in $request.routes) { $prefixes[[string]$route.prefix] = $true }
    foreach ($existing in Get-NetRoute -PolicyStore ActiveStore) {
        if ($prefixes.ContainsKey([string]$existing.DestinationPrefix) -and $existing.RouteMetric -ne 53717) {
            throw ('Route prefix is already owned: ' + $existing.DestinationPrefix)
        }
    }
    exit
}
# One native call per entry inside one process; never launch a process per CIDR.
Add-Type -TypeDefinition @'
using System;
using System.Net;
using System.Runtime.InteropServices;
public static class SempreRoutes {
    [StructLayout(LayoutKind.Sequential)]
    public struct Address {
        public ushort Family, Port;
        public uint Flow;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst=16)] public byte[] Ip;
        public uint Scope;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct Prefix { public Address Address; public byte Length; }
    [StructLayout(LayoutKind.Sequential)]
    public struct Row {
        public ulong Luid;
        public uint Index;
        public Prefix Prefix;
        public Address NextHop;
        public byte SitePrefixLength;
        public uint ValidLifetime, PreferredLifetime, Metric, Protocol;
        public byte Loopback, Autoconfigure, Publish, Immortal;
        public uint Age, Origin;
    }
    [DllImport("iphlpapi.dll")] static extern void InitializeIpForwardEntry(ref Row row);
    [DllImport("iphlpapi.dll")] static extern uint GetIpForwardEntry2(ref Row row);
    [DllImport("iphlpapi.dll")] static extern uint CreateIpForwardEntry2(ref Row row);
    [DllImport("iphlpapi.dll")] static extern uint DeleteIpForwardEntry2(ref Row row);
    static Address Parse(string value) {
        IPAddress ip = IPAddress.Parse(value);
        byte[] bytes = ip.GetAddressBytes();
        Address result = new Address { Ip = new byte[16] };
        if (bytes.Length == 4) { result.Family = 2; result.Flow = BitConverter.ToUInt32(bytes, 0); }
        else { result.Family = 23; result.Ip = bytes; result.Scope = (uint)ip.ScopeId; }
        return result;
    }
    public static void Change(string prefix, uint index, string gateway, bool add) {
        string[] parts = prefix.Split('/');
        Row key = new Row();
        InitializeIpForwardEntry(ref key);
        key.Index = index;
        key.Prefix = new Prefix { Address = Parse(parts[0]), Length = byte.Parse(parts[1]) };
        key.NextHop = Parse(String.IsNullOrEmpty(gateway) ? (parts[0].Contains(":") ? "::" : "0.0.0.0") : gateway);
        Row existing = key;
        uint found = GetIpForwardEntry2(ref existing);
        if (found == 0) {
            if (existing.Metric != 53717 || existing.Protocol != 3) throw new Exception("Route ownership changed: " + prefix);
            if (add) return;
            uint deleted = DeleteIpForwardEntry2(ref existing);
            if (deleted != 0 && deleted != 1168) throw new Exception("DeleteIpForwardEntry2: " + deleted);
            return;
        }
        if (found != 1168 && found != 87) throw new Exception("GetIpForwardEntry2: " + found);
        if (!add) return;
        key.Metric = 53717; key.Protocol = 3;
        key.ValidLifetime = uint.MaxValue; key.PreferredLifetime = uint.MaxValue;
        uint created = CreateIpForwardEntry2(ref key);
        if (created != 0) throw new Exception("CreateIpForwardEntry2: " + created + " " + prefix);
    }
}
'@
foreach ($route in $request.routes) {
    [SempreRoutes]::Change([string]$route.prefix, [uint32]$route.endpoint.index, [string]$route.endpoint.gateway, [bool]$request.add)
}
