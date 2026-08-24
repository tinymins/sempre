import { describe, expect, it } from "vitest";
import type { SubscriptionConfigurationContext, SubscriptionEditorConfig } from "@/lib/types";
import { recommendedEditorDefaults } from "./ProxySubscribeModel";

const common: SubscriptionEditorConfig = {
	rule_list: "{}", group: "[]", filter: "[]", custom_config: "[]",
	dns_config: "udp", private_access_config: "", servers: "[]",
};

describe("recommendedEditorDefaults", () => {
	it("selects the recommendation for the configured core", () => {
		const context = {
			key: "sing-box", platform: "windows",
			target: { core: "sing-box", version: "1.13.0", compiler_target: { core: "sing-box", format: "sing-box-v13-windows" }, key: "sing-box" },
			capabilities: { features: [], enum_values: {}, protocols: [] },
		} satisfies SubscriptionConfigurationContext;
		const selected = recommendedEditorDefaults({ ...common, by_core: { "sing-box": { ...common, dns_config: "tls-853" } } }, context);
		expect(selected.dns_config).toBe("tls-853");
	});

	it("falls back to the common recommendation without a configured core", () => {
		const context = { key: "common", platform: "windows", capabilities: { features: [], enum_values: {}, protocols: [] } } satisfies SubscriptionConfigurationContext;
		expect(recommendedEditorDefaults(common, context)).toBe(common);
	});
});
