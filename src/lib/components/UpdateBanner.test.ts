// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import UpdateBanner from "$lib/components/UpdateBanner.svelte";
import type { UpdateBanner as Banner } from "$lib/update-banner";

function mount(banner: Banner, installError: string | null = null) {
  const onUpdate = vi.fn();
  const onRetry = vi.fn();
  const onDismiss = vi.fn();
  render(UpdateBanner, { banner, installError, onUpdate, onRetry, onDismiss });
  return { onUpdate, onRetry, onDismiss };
}

describe("UpdateBanner", () => {
  it("names the version and installs it on click", async () => {
    const { onUpdate, onDismiss } = mount({ state: "available", version: "1.0.2" });
    expect(screen.getByText("Version 1.0.2 is available")).toBeInTheDocument();

    (screen.getByRole("button", { name: "Update" }) as HTMLButtonElement).click();
    expect(onUpdate).toHaveBeenCalledOnce();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("dismisses without installing", async () => {
    const { onUpdate, onDismiss } = mount({ state: "available", version: "1.0.2" });
    (screen.getByRole("button", { name: "Dismiss update notice" }) as HTMLButtonElement).click();
    expect(onDismiss).toHaveBeenCalledOnce();
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("offers nothing to click while installing", () => {
    mount({ state: "downloading" });
    expect(screen.getByText("Installing update…")).toBeInTheDocument();
    // No Update to press twice and no dismiss to hide an install in flight.
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("keeps the failure visible and retryable", async () => {
    const { onRetry } = mount({ state: "failed" }, "network unreachable");
    const text = screen.getByText("Update failed");
    expect(text).toBeInTheDocument();
    // Full message in the tooltip: the bar is 440px wide.
    expect(text).toHaveAttribute("title", "network unreachable");

    (screen.getByRole("button", { name: "Retry" }) as HTMLButtonElement).click();
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("announces itself to assistive tech without stealing focus", () => {
    mount({ state: "available", version: "1.0.2" });
    expect(screen.getByRole("status")).toBeInTheDocument();
  });
});
