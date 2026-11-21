import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "./App";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const appInfo = { name: "Screenstory", version: "0.2.0", runtime: "Tauri" };
const report = { dependencies: [{
  id: "ffmpeg", feature: "X11 desktop recording and video processing", status: "available",
  version: "ffmpeg version 6.1.1", impact: "Desktop recording is available.", canInstall: false, remediation: null,
}] };
const project = { id: "p12345-123", name: "Demo project", createdAt: "1730000000000", updatedAt: "1730000000000", recordingsCount: 1 };
const recording = { id: "recording-1", fileName: "recording-1.mp4", createdAt: "1730000000000", durationSeconds: 12 };
const timeline = {
  schemaVersion: 1, timelineId: "t12345-77", recordingId: "recording-1",
  sourceFileName: "recording-1.mp4", sourceDurationMs: 12000, sourceAnalysisRunId: "a12345-77",
  revision: 1, createdAtMs: 100, updatedAtMs: 100, status: "draft", language: "en",
  steps: [{
    id: "step-1", title: "Open settings", description: "Open the settings page.",
    startMs: 0, endMs: 1000, origin: "ai", evidenceFrameIds: ["frame-000"],
    confidence: 0.9, evidenceKind: "observed",
  }],
  zoomSegments: [], annotations: [],
};

function standard(command: string) {
  switch (command) {
    case "get_app_info": return Promise.resolve(appInfo);
    case "get_gemini_key_status": return Promise.resolve({ configured: false });
    case "check_dependencies": return Promise.resolve(report);
    case "get_recording_status": return Promise.resolve({ isRecording: false, fileName: null, startedAtMs: null });
    case "list_projects": return Promise.resolve([]);
    case "get_active_project": return Promise.resolve(null);
    case "list_recordings": return Promise.resolve([]);
    case "get_timeline_for_recording": return Promise.resolve(null);
    case "create_project": return Promise.resolve(project);
    case "open_project": return Promise.resolve(project);
    case "start_recording": return Promise.resolve({ fileName: "recording-1.mp4", startedAtMs: Date.now() });
    case "stop_recording": return Promise.resolve({ id: "recording-1", fileName: "recording-1.mp4", createdAt: "1730000000000", durationSeconds: 12 });
    default: return Promise.reject(new Error("Unexpected command: " + command));
  }
}

describe("Screenstory desktop application", () => {
  beforeEach(() => {
    cleanup();
    invoke.mockReset();
    invoke.mockImplementation((command: string) => standard(command));
  });

  it("loads the local workspace and offers project creation", async () => {
    render(<App />);
    expect(await screen.findByText("Create a clear story.")).toBeTruthy();
    expect(screen.getByRole("button", { name: /New project/ })).toBeTruthy();
    expect(await screen.findByText("Native core connected")).toBeTruthy();
  });

  it("creates a project through the native command and opens the new workspace", async () => {
    let created = false;
    invoke.mockImplementation((command: string) => {
      if (command === "create_project") { created = true; return Promise.resolve(project); }
      if (command === "list_projects") return Promise.resolve(created ? [project] : []);
      if (command === "get_active_project") return Promise.resolve(created ? project : null);
      return standard(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /New project/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "Demo project" } });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_project", { name: "Demo project" }));
    expect(await screen.findByText("Capture your workflow")).toBeTruthy();
  });

  it("starts and stops a recording in an open project", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      return standard(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /Start recording/ }));
    expect(await screen.findByRole("button", { name: /Stop & save/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Stop & save/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("stop_recording"));
  });

  it("opens, edits, and saves a versioned timeline revision", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      if (command === "list_recordings") return Promise.resolve([recording]);
      if (command === "get_timeline_for_recording") return Promise.resolve(timeline);
      if (command === "save_timeline") return Promise.resolve({ ...timeline, revision: 2 });
      return standard(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Open timeline" }));
    const titleInput = await screen.findByRole("textbox", { name: "Step title" });
    fireEvent.change(titleInput, { target: { value: "Open account settings" } });
    fireEvent.click(screen.getByRole("button", { name: "Save revision" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_timeline", {
      timeline: { ...timeline, steps: [{ ...timeline.steps[0], title: "Open account settings" }] },
      expectedRevision: 1,
    }));
    expect(await screen.findByText(/Timeline saved · revision 2/)).toBeTruthy();
  });

  it("surfaces a native startup/storage error without crashing", async () => {
    invoke.mockImplementation((command: string) => command === "get_app_info"
      ? Promise.resolve(appInfo)
      : command === "list_projects"
        ? Promise.reject(new Error("storage unavailable"))
        : standard(command));
    render(<App />);
    expect(await screen.findByRole("alert")).toBeTruthy();
  });
});
