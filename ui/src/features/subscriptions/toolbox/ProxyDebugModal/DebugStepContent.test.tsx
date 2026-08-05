import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AcmeContentBoundary } from "@/components/AcmeContentBoundary";
import { I18nProvider } from "@/lib/i18n";
import type { ProxyDebugStep } from "../types/proxy";
import { ManualServersStepContent } from "./DebugStepContent";

function renderStep(step: Extract<ProxyDebugStep, { type: "manual-servers" }>) {
  return render(
    <I18nProvider>
      <AcmeContentBoundary>
        <ManualServersStepContent step={step} />
      </AcmeContentBoundary>
    </I18nProvider>,
  );
}

describe("ManualServersStepContent", () => {
  afterEach(() => {
    cleanup();
    localStorage.removeItem("sempre.locale");
  });

  it("shows manual and custom server origins", () => {
    localStorage.setItem("sempre.locale", "en");
    renderStep({
      type: "manual-servers",
      data: {
        count: 2,
        nodes: [
          { name: "Manual", type: "socks5", server: "127.0.0.1", port: 1080, sourceIndex: 0, sourceUrl: "manual", raw: {} },
          { name: "Custom", type: "http", server: "127.0.0.2", port: 8080, sourceIndex: 0, sourceUrl: "custom-node:custom-1", raw: {} },
        ],
      },
    });

    expect(screen.getByText("Manual config")).toBeInTheDocument();
    expect(screen.getByText("Custom server")).toBeInTheDocument();
    expect(screen.queryByText("No local servers")).not.toBeInTheDocument();
  });

  it("uses the local server empty state", () => {
    localStorage.setItem("sempre.locale", "en");
    renderStep({ type: "manual-servers", data: { count: 0, nodes: [] } });

    expect(screen.getByText("No local servers")).toBeInTheDocument();
  });
});
