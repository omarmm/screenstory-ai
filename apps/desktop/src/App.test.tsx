import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import App from "./App";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

const appInfo = { name: "Screenstory", version: "0.1.0", runtime: "Tauri" };
const dependencyReport = {
  dependencies: [{
    id: "ffmpeg", feature: "Recording and video processing", status: "missing",
    version: null, impact: "FFmpeg is required for video features.",
    canInstall: true, remediation: "Install FFmpeg from system repositories.",
  }],
};

describe("Screenstory desktop shell", () => {
  beforeEach(() => {
    cleanup();
    invoke.mockReset();
    invoke.mockImplementation((command: string) => {
      if (command === "get_app_info") return Promise.resolve(appInfo);
      if (command === "check_dependencies") return Promise.resolve(dependencyReport);
      return Promise.reject(new Error("Unexpected command"));
    });
  });

  it("shows the honest empty project state and missing feature dependency", async () => {
    render(<App />);
    expect(screen.getByText("Your workspace is ready")).toBeTruthy();
    expect(screen.getByRole("button", { name: /New project/ })).toBeDisabled();
    expect(await screen.findByText("FFmpeg")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Install FFmpeg" })).toBeTruthy();
  });

  it("shows the native core version after a successful bridge call", async () => {
    render(<App />);
    expect(await screen.findByText("v0.1.0 · Tauri")).toBeTruthy();
  });

  it("keeps the shell available when dependency detection fails", async () => {
    invoke.mockImplementation((command: string) => command === "get_app_info"
      ? Promise.resolve(appInfo)
      : Promise.reject(new Error("probe failed")));
    render(<App />);
    expect(await screen.findByText("Your workspace is ready")).toBeTruthy();
    expect(await screen.findByRole("status")).toBeTruthy();
  });
});
