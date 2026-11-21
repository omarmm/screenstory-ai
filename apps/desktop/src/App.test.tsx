import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import App from "./App";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

describe("Screenstory desktop shell", () => {
  beforeEach(() => {
    cleanup();
    invoke.mockReset();
  });

  it("shows the honest empty project state", () => {
    invoke.mockReturnValue(new Promise(() => undefined));
    render(<App />);
    expect(screen.getByText("Your workspace is ready")).toBeTruthy();
    expect(screen.getByRole("button", { name: /New project/ })).toBeDisabled();
  });

  it("shows the native core version after a successful bridge call", async () => {
    invoke.mockResolvedValue({ name: "Screenstory", version: "0.1.0", runtime: "Tauri" });
    render(<App />);
    expect(await screen.findByText("v0.1.0 · Tauri")).toBeTruthy();
  });
});