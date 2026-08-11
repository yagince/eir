// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import Auth from "$lib/components/Auth.svelte";

/// Shape of what `start_device_flow` hands back on the Rust side. Only
/// user_code and verification_uri reach the markup; the rest are carried
/// through so the fixture matches the real payload.
const DEVICE_CODE = {
  user_code: "WDJB-MJHT",
  verification_uri: "https://github.com/login/device",
  device_code: "3584d83530557fdd1f46af8289938c8ef79f9dc5",
  interval: 5,
  expires_in: 900,
};

/// Every callback Auth can reach in either phase. Both mounts pass all three so
/// a test can assert that the control it pressed fired and the others did not —
/// that also catches a handler leaking into the phase it does not belong to.
function handlers() {
  return {
    onSignIn: vi.fn(),
    onCopyCode: vi.fn(),
    onReopenVerification: vi.fn(),
  };
}

function mountIdle(error: string | null = null) {
  const on = handlers();
  render(Auth, { phase: "idle", error, ...on });
  return on;
}

function mountPending(overrides: { copied?: boolean; error?: string | null } = {}) {
  const on = handlers();
  render(Auth, {
    phase: "pending",
    deviceCode: DEVICE_CODE,
    copied: false,
    error: null,
    ...overrides,
    ...on,
  });
  return on;
}

describe("Auth sign-in phase", () => {
  it("asks for sign-in and starts the device flow on click", () => {
    const { onSignIn, onCopyCode, onReopenVerification } = mountIdle();
    expect(screen.getByText("Sign in to start tracking your PRs and Issues.")).toBeInTheDocument();

    screen.getByRole("button", { name: "Sign in with GitHub" }).click();
    expect(onSignIn).toHaveBeenCalledOnce();
    expect(onCopyCode).not.toHaveBeenCalled();
    expect(onReopenVerification).not.toHaveBeenCalled();
  });

  it("shows the failure that knocked the flow back to idle", () => {
    // signIn() sets error and flips phase to "idle" together, so this is the
    // only place a device-flow error is ever visible.
    mountIdle("invalid_client");
    expect(screen.getByText("invalid_client")).toBeInTheDocument();
  });

  it("keeps the error slot empty when there is nothing to report", () => {
    mountIdle();
    expect(screen.queryByText("invalid_client")).not.toBeInTheDocument();
    // The CTA is the only control: nothing to copy, nothing to reopen yet.
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("offers none of the device-code UI before a code exists", () => {
    mountIdle();
    expect(screen.queryByText("Enter this code on GitHub:")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open GitHub again" })).not.toBeInTheDocument();
    expect(screen.queryByText("Waiting for authorization…")).not.toBeInTheDocument();
  });
});

describe("Auth device-code phase", () => {
  it("renders the code the user has to type into GitHub", () => {
    mountPending();
    expect(screen.getByText("Enter this code on GitHub:")).toBeInTheDocument();
    // The code is the button's own label, so this fails if the markup ever
    // stops interpolating user_code (an empty button would still render).
    const code = screen.getByRole("button", { name: DEVICE_CODE.user_code });
    expect(code).toHaveAttribute("title", "Click to copy");
    expect(screen.getByText("Waiting for authorization…")).toBeInTheDocument();
  });

  it("copies from the code button and only that", () => {
    const { onCopyCode, onReopenVerification, onSignIn } = mountPending();
    screen.getByRole("button", { name: DEVICE_CODE.user_code }).click();
    expect(onCopyCode).toHaveBeenCalledOnce();
    expect(onReopenVerification).not.toHaveBeenCalled();
    expect(onSignIn).not.toHaveBeenCalled();
  });

  it("reopens GitHub with the verification URL from the payload", () => {
    const { onReopenVerification, onCopyCode } = mountPending();
    screen.getByRole("button", { name: "Open GitHub again" }).click();
    // The URL is passed by the component, not read from the parent's scope, so
    // pin the argument and not just the call.
    expect(onReopenVerification).toHaveBeenCalledExactlyOnceWith(DEVICE_CODE.verification_uri);
    expect(onCopyCode).not.toHaveBeenCalled();
  });

  it("prompts for a tap while the code is not on the clipboard", () => {
    mountPending({ copied: false });
    expect(screen.getByText("Tap to copy")).toBeInTheDocument();
    expect(screen.queryByText("✓ Copied to clipboard")).not.toBeInTheDocument();
  });

  it("confirms the copy once the clipboard write lands", () => {
    // copied also drives the un-pinning of the popup in the parent, so the
    // confirmation is the user's only signal that focus can now move away.
    mountPending({ copied: true });
    expect(screen.getByText("✓ Copied to clipboard")).toBeInTheDocument();
    expect(screen.queryByText("Tap to copy")).not.toBeInTheDocument();
  });

  it("offers exactly the copy and reopen controls, not sign-in", () => {
    mountPending();
    const labels = screen.getAllByRole("button").map((b) => b.textContent?.trim());
    expect(labels).toEqual([DEVICE_CODE.user_code, "Open GitHub again"]);
    expect(screen.queryByRole("button", { name: "Sign in with GitHub" })).not.toBeInTheDocument();
  });

  it("shows a copy failure without leaving the phase", () => {
    // copyCode() sets `error` while phase stays "pending", so this branch is the
    // only place a rejected clipboard write can surface. It used to render
    // nothing, leaving the status line on "Tap to copy" as if the click had not
    // happened.
    mountPending({ error: "copy failed: NotAllowedError" });
    expect(screen.getByText("copy failed: NotAllowedError")).toBeInTheDocument();
    // The code is still there to copy by hand.
    expect(screen.getByRole("button", { name: DEVICE_CODE.user_code })).toBeInTheDocument();
  });

  it("shows no error paragraph when there is nothing wrong", () => {
    mountPending();
    expect(document.querySelector(".error")).not.toBeInTheDocument();
  });
});
