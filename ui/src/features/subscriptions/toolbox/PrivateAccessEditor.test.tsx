import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/lib/i18n";
import { emptyConnector, serializeConfig } from "./PrivateAccessConfig";
import PrivateAccessEditor from "./PrivateAccessEditor";

describe("PrivateAccessEditor WireGuard import", () => {
  it("enables import only for WireGuard and fills the selected connector", async () => {
    localStorage.setItem("sempre.locale", "zh-CN");
    const onChange = vi.fn();
    const value = serializeConfig(true, [
      { ...emptyConnector(), tag: "wg", routeCidrs: "10.0.0.0/8" },
      { ...emptyConnector(), tag: "proxy", type: "vmess" },
    ]);

    render(
      <QueryClientProvider client={new QueryClient()}>
        <I18nProvider>
          <PrivateAccessEditor value={value} onChange={onChange} profileId="profile" />
        </I18nProvider>
      </QueryClientProvider>,
    );

    const importButtons = screen.getAllByRole("button", { name: "一键导入配置" });
    expect(importButtons[0]).toBeEnabled();
    expect(importButtons[1]).toBeDisabled();
    fireEvent.click(importButtons[0]);

    fireEvent.change(screen.getByRole("textbox", { name: "WireGuard 配置" }), {
      target: {
        value: `[Interface]\nPrivateKey=private=\nAddress=10.3.8.10/32\nDNS=10.3.7.1\n[Peer]\nPublicKey=public=\nAllowedIPs=0.0.0.0/0, ::/0\nEndpoint=14.90.103.42:31088\nPersistentKeepAlive=25`,
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "确定" }));

    const emitted = JSON.parse(onChange.mock.lastCall?.[0] as string);
    expect(emitted.connectors[0]).toMatchObject({
      tag: "wg",
      routes: { ipCidrs: ["10.0.0.0/8"] },
      endpoint: {
        address: ["10.3.8.10/32"],
        privateKey: "private=",
        peers: [
          {
            address: "14.90.103.42",
            port: 31088,
            publicKey: "public=",
            allowedIps: ["0.0.0.0/0", "::/0"],
          },
        ],
      },
      dns: [{ server: "10.3.7.1", serverPort: 53 }],
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});
