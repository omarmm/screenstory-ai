import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "./App";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args), convertFileSrc: (path: string) => "asset://localhost/" + path }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => undefined) }));

const appInfo = { name: "Screenstory", version: "0.2.0", runtime: "Tauri" };
const report = { dependencies: [
  { id: "ffmpeg", feature: "FFmpeg and FFprobe · recording and video analysis", status: "available", version: "ffmpeg version 6.1.1", impact: "Recording and video analysis are available.", packages: [], canInstall: false, remediation: null },
  { id: "ffmpeg-export", feature: "MP4 export · H.264 and AAC encoders", status: "available", version: "libx264 and AAC", impact: "MP4 video and audio encoders are available.", packages: [], canInstall: false, remediation: null },
  { id: "zoom-effects", feature: "Zoom effects · FFmpeg zoompan filter", status: "available", version: "zoompan", impact: "Zoompan is available.", packages: [], canInstall: false, remediation: null },
  { id: "caption-rendering", feature: "Caption rendering · FFmpeg ASS/libass filter", status: "available", version: "ass/libass", impact: "Caption rendering is available.", packages: [], canInstall: false, remediation: null },
  { id: "caption-fonts", feature: "Caption fonts · Noto Sans and Noto Sans Arabic", status: "available", version: "Noto fonts", impact: "Caption fonts are available.", packages: [], canInstall: false, remediation: null },
  { id: "arabic-shaping", feature: "Arabic caption shaping · HarfBuzz/libass", status: "available", version: "complex", impact: "Arabic shaping is available.", packages: [], canInstall: false, remediation: null },
  { id: "wayland-capture", feature: "Wayland capture · XDG Desktop Portal and PipeWire", status: "available", version: "X11", impact: "X11 session active.", packages: [], canInstall: false, remediation: null },
] };
const project = { id: "p12345-123", name: "Demo project", createdAt: "1730000000000", updatedAt: "1730000000000", recordingsCount: 1 };
const recording = { id: "recording-1", fileName: "recording-1.mp4", createdAt: "1730000000000", durationSeconds: 12, captureSourceName: "Entire desktop", hasTimeline: true };
const captureSource = { id: "capture-test-display", name: "Entire desktop · 1920 × 1080", sourceType: "display", backend: "x11", width: 1920, height: 1080, isAvailable: true, isMinimized: false, appHint: null };
const timeline = {
  schemaVersion: 2, timelineId: "t12345-77", recordingId: "recording-1",
  sourceFileName: "recording-1.mp4", sourceDurationMs: 12000, sourceAnalysisRunId: "a12345-77",
  revision: 1, createdAtMs: 100, updatedAtMs: 100, status: "draft", language: "en",
  steps: [{
    id: "step-1", title: "Open settings", description: "Open the settings page.",
    startMs: 0, endMs: 1000, origin: "ai", evidenceFrameIds: ["frame-000"],
    confidence: 0.9, evidenceKind: "observed",
  }],
  zoomSegments: [{
    id: "zoom-1", startMs: 1000, endMs: 2500, x: 0.2, y: 0.2, width: 0.4, height: 0.4,
    targetScale: 1.5, rationale: "Focus the settings button.", evidenceFrameIds: ["frame-000"],
    origin: "ai", approvalStatus: "pending", transitionDurationMs: 250,
  }],
  annotations: [],
};

function standard(command: string) {
  switch (command) {
    case "get_app_info": return Promise.resolve(appInfo);
    case "get_gemini_key_status": return Promise.resolve({ configured: false });
    case "check_dependencies": return Promise.resolve(report);
    case "get_recording_status": return Promise.resolve({ isRecording: false, fileName: null, startedAtMs: null, warning: null });
    case "list_capture_sources": return Promise.resolve([captureSource]);
    case "list_projects": return Promise.resolve([]);
    case "get_active_project": return Promise.resolve(null);
    case "list_recordings": return Promise.resolve([]);
    case "get_timeline_for_recording": return Promise.resolve(null);
    case "create_project": return Promise.resolve(project);
    case "open_project": return Promise.resolve(project);
    case "start_recording": return Promise.resolve({ fileName: "recording-1.mp4", startedAtMs: Date.now() });
    case "stop_recording": return Promise.resolve({ id: "recording-1", fileName: "recording-1.mp4", createdAt: "1730000000000", durationSeconds: 12 });
    case "start_render": return Promise.resolve({ runId: "r1730000000000-123", timelineId: timeline.timelineId, timelineRevision: timeline.revision, outputFileName: "Screenstory-tutorial-r1730000000000-123.mp4" });
    case "get_render_status": return Promise.resolve({
      runId: "r1730000000000-123", recordingId: "recording-1", timelineId: timeline.timelineId,
      timelineRevision: timeline.revision, status: "completed", progress: 100,
      outputFileName: "Screenstory-tutorial-r1730000000000-123.mp4",
      outputPath: "/home/test/Videos/Screenstory/Projects/p12345-123/exports/tutorial.mp4",
      outputSizeBytes: 2 * 1024 * 1024, durationSeconds: 12, errorCode: null,
      rendererVersion: "ffmpeg test", hasAudio: true, annotationCount: 1, activeZoomCount: 1,
    });
    default: return Promise.reject(new Error("Unexpected command: " + command));
  }
}

describe("Screenstory desktop application", () => {
  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
    invoke.mockReset();
    invoke.mockImplementation((command: string) => standard(command));
  });

  it("installs a missing runtime dependency only when the user invokes recording", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const missingReport = {
      dependencies: report.dependencies.map((dependency) => dependency.id === "ffmpeg"
        ? { ...dependency, status: "missing", version: null, packages: ["ffmpeg"], impact: "Recording is unavailable.", canInstall: true, remediation: "Screenstory can install FFmpeg." }
        : dependency)
    };
    invoke.mockImplementation((command: string) => {
      if (command === "check_dependencies") return Promise.resolve(missingReport);
      if (command === "install_dependency") return Promise.resolve(report);
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      return standard(command);
    });
    render(<App />);
    expect(await screen.findByText("Demo project")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Install automatically" })).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("install_dependency", { dependencyId: "ffmpeg" });

    fireEvent.click(await screen.findByRole("button", { name: /Set up & start recording/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("install_dependency", { dependencyId: "ffmpeg" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_recording", { captureSourceId: captureSource.id }));
    expect(confirm).toHaveBeenCalled();
    expect(await screen.findByRole("button", { name: /Stop & save/ })).toBeTruthy();
    confirm.mockRestore();
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
    expect((await screen.findByLabelText("Capture source") as HTMLSelectElement).value).toBe(captureSource.id);
    fireEvent.click(await screen.findByRole("button", { name: /Start recording/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_recording", { captureSourceId: captureSource.id }));
    expect(await screen.findByRole("button", { name: /Stop & save/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Stop & save/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("stop_recording"));
  });

  it("disables minimized windows and allows refreshing capture sources", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      if (command === "list_recordings") return Promise.resolve([recording]);
      if (command === "list_capture_sources") return Promise.resolve([
        captureSource,
        { ...captureSource, id: "capture-minimized-window", name: "Minimized browser", sourceType: "window", isAvailable: false, isMinimized: true },
      ]);
      return standard(command);
    });
    render(<App />);
    const select = await screen.findByLabelText("Capture source") as HTMLSelectElement;
    expect((select.querySelector('option[value="capture-minimized-window"]') as HTMLOptionElement | null)?.disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Refresh sources" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_capture_sources"));
  });

  it("explains that a timeline is unavailable until analysis succeeds and a draft is converted", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      if (command === "list_recordings") return Promise.resolve([{ ...recording, hasTimeline: false }]);
      return standard(command);
    });
    render(<App />);
    expect(await screen.findByText("No timeline yet")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Open timeline" })).toBeNull();
  });

  it("deletes a recording only after explicit confirmation and refreshes the library", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    let deleted = false;
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      if (command === "list_recordings") return Promise.resolve(deleted ? [] : [recording]);
      if (command === "delete_recording") { deleted = true; return Promise.resolve(undefined); }
      return standard(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Delete recording recording-1.mp4" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_recording", { recordingId: "recording-1" }));
    expect(confirm).toHaveBeenCalled();
    expect(await screen.findByText("No recordings yet")).toBeTruthy();
    expect(await screen.findByText(/and its managed data were deleted/)).toBeTruthy();
    confirm.mockRestore();
  });

  it("deletes a project only after explicit confirmation and returns to the overview", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    let deleted = false;
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve(deleted ? [] : [project]);
      if (command === "get_active_project") return Promise.resolve(deleted ? null : project);
      if (command === "delete_project") { deleted = true; return Promise.resolve(undefined); }
      return standard(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Delete project Demo project" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_project", { projectId: project.id }));
    expect(confirm).toHaveBeenCalled();
    expect(await screen.findByText("Create a clear story.")).toBeTruthy();
    expect(await screen.findByText(/and its managed files were deleted/)).toBeTruthy();
    confirm.mockRestore();
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

  it("accepts a zoom suggestion and saves a bilingual annotation", async () => {
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
    fireEvent.click(await screen.findByRole("button", { name: "Accept zoom" }));
    fireEvent.click(screen.getByRole("button", { name: /Add caption/ }));
    fireEvent.change(screen.getByLabelText("Caption text"), { target: { value: "اضغط على الإعدادات" } });
    fireEvent.click(screen.getByRole("button", { name: "Save revision" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_timeline", expect.objectContaining({
      expectedRevision: 1,
      timeline: expect.objectContaining({
        schemaVersion: 2,
        zoomSegments: [expect.objectContaining({ approvalStatus: "accepted", transitionDurationMs: 250 })],
        annotations: [expect.objectContaining({ text: "اضغط على الإعدادات", language: "mixed", direction: "auto", origin: "manual" })],
      }),
    })));
  });
  it("exports only the saved timeline revision and displays the verified MP4 result", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_projects") return Promise.resolve([project]);
      if (command === "get_active_project") return Promise.resolve(project);
      if (command === "list_recordings") return Promise.resolve([recording]);
      if (command === "get_timeline_for_recording") return Promise.resolve(timeline);
      return standard(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Open timeline" }));
    fireEvent.click(await screen.findByRole("button", { name: "Export MP4" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_render", {
      recordingId: timeline.recordingId,
      expectedRevision: timeline.revision,
    }));
    expect(await screen.findByText("Your tutorial is ready")).toBeTruthy();
    expect(screen.getByText(/tutorial\.mp4/)).toBeTruthy();
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
