import type { LinuxNetworkInventory, SubscriptionConfigurationContext } from '@/lib/types'
export function dnsOptions(context: SubscriptionConfigurationContext, inventory?: LinuxNetworkInventory) {
  return { features: ['windows', 'macos'].includes(context.platform) ? context.capabilities.features.filter(feature => feature !== 'dns.system_takeover') : context.capabilities.features, systemDnsListenHostOptions: systemDnsListenOptions(inventory) }
}

function systemDnsListenOptions(inventory?: LinuxNetworkInventory) {
	const options: Array<{ value: string; label: string }> = [];
	const seen = new Set(["127.0.0.1", "0.0.0.0"]);
	for (const item of inventory?.interfaces ?? []) {
		if (!item.up) continue;
		for (const value of item.addresses) {
			const host = value.split("/")[0]?.trim();
			if (!host || seen.has(host) || !isIPv4Address(host)) continue;
			seen.add(host);
			options.push({ value: host, label: `${host} · ${item.name}` });
		}
	}
	return options;
}

function isIPv4Address(value: string) {
	const parts = value.split(".");
	return parts.length === 4 && parts.every((part) => {
		if (!/^\d+$/.test(part)) return false;
		const number = Number(part);
		return number >= 0 && number <= 255 && String(number) === String(Number.parseInt(part, 10));
	});
}
