import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

type AppInfo = { name: string; version: string; runtime: string };
type Dependency = { id: string; feature: string; status: "available" | "missing" | "unsupported" | "error"; version: string | null; impact: string; canInstall: boolean; remediation: string | null };
type DependencyReport = { dependencies: Dependency[] };
type Project = { id: string; name: string; createdAt: string; updatedAt: string; recordingsCount: number };
type Recording = { id: string; fileName: string; createdAt: string; durationSeconds: number };
type CaptureStatus = { isRecording: boolean; fileName: string | null; startedAtMs: number | null };
type RenderStarted = { runId: string; timelineId: string; timelineRevision: number; outputFileName: string };
type RenderStatus = {
  runId: string; recordingId: string; timelineId: string; timelineRevision: number; status: string;
  progress: number; outputFileName: string; outputPath: string | null; outputSizeBytes: number;
  durationSeconds: number; errorCode: string | null; rendererVersion: string;
  hasAudio: boolean; annotationCount: number; activeZoomCount: number;
};
type AnalysisStatus = {
  runId: string; recordingId: string; sourceFileName: string; status: string;
  completedFrames: number; totalFrames: number; totalBytes: number;
  durationSeconds: number | null; provider: string; model: string;
  errorCode: string | null; workflowDescription: string; language: string;
};
type AnalysisDraft = {
  schemaVersion: number; summary: string; status: string; language: string;
  steps: { id: string; title: string; description: string; startMs: number; endMs: number; confidence: number; evidenceKind: string; evidenceFrameIds: string[] }[];
  zoomSuggestions: { id: string; startMs: number; endMs: number; x: number; y: number; width: number; height: number; targetScale: number; rationale: string; evidenceFrameIds: string[] }[];
};
type TimelineStep = {
  id: string; title: string; description: string; startMs: number; endMs: number;
  origin: "ai" | "manual"; evidenceFrameIds: string[]; confidence: number | null; evidenceKind: string | null;
};
type TimelineZoom = {
  id: string; startMs: number; endMs: number; x: number; y: number; width: number; height: number;
  targetScale: number; rationale: string; evidenceFrameIds: string[]; origin: string;
  approvalStatus: "pending" | "accepted" | "rejected"; transitionDurationMs: number;
};
type TimelineAnnotation = {
  id: string; text: string; language: "ar" | "en" | "mixed"; startMs: number; endMs: number;
  x: number; y: number; maxWidth: number; fontSize: number; foreground: string; background: string;
  alignment: "left" | "center" | "right"; direction: "rtl" | "ltr" | "auto"; origin: "manual";
};
type TimelineDocument = {
  schemaVersion: number; timelineId: string; recordingId: string; sourceFileName: string;
  sourceDurationMs: number; sourceAnalysisRunId: string; revision: number;
  createdAtMs: number; updatedAtMs: number; status: string; language: string;
  steps: TimelineStep[];
  zoomSegments: TimelineZoom[];
  annotations: TimelineAnnotation[];
};

function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  return [hours, minutes, remainder].map((part) => String(part).padStart(2, "0")).join(":");
}
function formatDate(value: string): string {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return "Recently";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(numeric));
}
function friendlyError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  return "Something went wrong. Please try again.";
}

export default function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [dependencies, setDependencies] = useState<DependencyReport | null>(null);
  const [capture, setCapture] = useState<CaptureStatus>({ isRecording: false, fileName: null, startedAtMs: null });
  const [clock, setClock] = useState(Date.now());
  const [projectName, setProjectName] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [geminiKeyConfigured, setGeminiKeyConfigured] = useState(false);
  const [geminiKeyInput, setGeminiKeyInput] = useState("");
  const [workflowDescription, setWorkflowDescription] = useState("");
  const [analysisLanguage, setAnalysisLanguage] = useState<"auto" | "en" | "ar">("auto");
  const [activeAnalysis, setActiveAnalysis] = useState<AnalysisStatus | null>(null);
  const [analysisDraft, setAnalysisDraft] = useState<AnalysisDraft | null>(null);
  const [analysisBusy, setAnalysisBusy] = useState(false);
  const [credentialStoreAvailable, setCredentialStoreAvailable] = useState(true);
  const [timeline, setTimeline] = useState<TimelineDocument | null>(null);
  const [timelineBusy, setTimelineBusy] = useState(false);
  const [savedTimelineSignature, setSavedTimelineSignature] = useState<string | null>(null);
  const [renderStatus, setRenderStatus] = useState<RenderStatus | null>(null);
  const [renderBusy, setRenderBusy] = useState(false);

  const ffmpeg = dependencies?.dependencies.find((item) => item.id === "ffmpeg");
  const elapsedSeconds = useMemo(() => capture.startedAtMs === null ? 0 : Math.floor((clock - capture.startedAtMs) / 1000), [capture.startedAtMs, clock]);
  const timelineDirty = !!timeline && savedTimelineSignature !== JSON.stringify(timeline);
  const renderInProgress = !!renderStatus && !["completed", "failed", "cancelled", "interrupted"].includes(renderStatus.status);

  async function refreshProjects() {
    const [allProjects, current] = await Promise.all([
      invoke<Project[]>("list_projects"),
      invoke<Project | null>("get_active_project"),
    ]);
    setProjects(allProjects);
    setActiveProject(current);
    setRecordings(current ? await invoke<Recording[]>("list_recordings") : []);
  }
  async function bootstrap() {
    setLoading(true);
    setError(null);
    try {
      const [info, report, currentCapture] = await Promise.all([
        invoke<AppInfo>("get_app_info"),
        invoke<DependencyReport>("check_dependencies"),
        invoke<CaptureStatus>("get_recording_status"),
      ]);
      setAppInfo(info);
      setDependencies(report);
      setCapture(currentCapture);
      const keyStatus = await invoke<{ configured: boolean }>("get_gemini_key_status").catch(() => {
        setCredentialStoreAvailable(false);
        return { configured: false };
      });
      setGeminiKeyConfigured(keyStatus.configured);
      await refreshProjects();
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => { void bootstrap(); }, []);
  useEffect(() => {
    if (!activeAnalysis?.runId || ["awaiting_cloud_consent", "completed", "failed", "cancelled"].includes(activeAnalysis.status)) return;
    const runId = activeAnalysis.runId;
    const timer = window.setInterval(() => {
      void invoke<AnalysisStatus>("get_analysis_status", { runId })
        .then((next) => setActiveAnalysis(next))
        .catch((reason) => setError(friendlyError(reason)));
    }, 900);
    return () => window.clearInterval(timer);
  }, [activeAnalysis?.runId, activeAnalysis?.status]);

  useEffect(() => {
    if (!activeAnalysis || activeAnalysis.status !== "completed" || analysisDraft) return;
    void invoke<AnalysisDraft>("get_analysis_draft", { runId: activeAnalysis.runId })
      .then(setAnalysisDraft)
      .catch((reason) => setError(friendlyError(reason)));
  }, [activeAnalysis?.runId, activeAnalysis?.status, analysisDraft]);

  useEffect(() => {
    if (!renderStatus?.runId || ["completed", "failed", "cancelled", "interrupted", "missing"].includes(renderStatus.status)) return;
    const runId = renderStatus.runId;
    const timer = window.setInterval(() => {
      void invoke<RenderStatus>("get_render_status", { runId })
        .then(setRenderStatus)
        .catch((reason) => setError(friendlyError(reason)));
    }, 700);
    return () => window.clearInterval(timer);
  }, [renderStatus?.runId, renderStatus?.status]);

  useEffect(() => {
    if (!capture.isRecording) return;
    const timer = window.setInterval(() => setClock(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [capture.isRecording]);

  async function createProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true); setError(null);
    try {
      const created = await invoke<Project>("create_project", { name: projectName });
      setActiveProject(created); setProjectName(""); setShowCreate(false);
      setNotice("Project “" + created.name + "” is ready.");
      await refreshProjects();
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setBusy(false); }
  }
  async function openProject(project: Project) {
    setBusy(true); setError(null);
    try {
      const opened = await invoke<Project>("open_project", { projectId: project.id });
      setActiveProject(opened); setActiveAnalysis(null); setAnalysisDraft(null); setTimeline(null); setSavedTimelineSignature(null); setRenderStatus(null); setNotice("Opened “" + opened.name + "”.");
      await refreshProjects();
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setBusy(false); }
  }
  async function saveGeminiKey() {
    setAnalysisBusy(true); setError(null);
    try {
      const status = await invoke<{ configured: boolean }>("save_gemini_api_key", { apiKey: geminiKeyInput });
      setGeminiKeyConfigured(status.configured);
      setGeminiKeyInput("");
      setCredentialStoreAvailable(true);
      setNotice("Gemini API key saved in your desktop credential store.");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setAnalysisBusy(false); }
  }
  async function deleteGeminiKey() {
    setAnalysisBusy(true); setError(null);
    try {
      await invoke("delete_gemini_api_key");
      setGeminiKeyConfigured(false);
      setNotice("Gemini API key removed from your desktop credential store.");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setAnalysisBusy(false); }
  }
  async function startAnalysis(recording: Recording) {
    setAnalysisBusy(true); setError(null); setNotice(null); setAnalysisDraft(null);
    try {
      const started = await invoke<{ runId: string; recordingId: string; fileName: string }>("start_analysis", {
        recordingId: recording.id,
        workflowDescription,
        language: analysisLanguage,
      });
      setActiveAnalysis({
        runId: started.runId, recordingId: started.recordingId, sourceFileName: started.fileName,
        status: "extracting_frames", completedFrames: 0, totalFrames: 0, totalBytes: 0,
        durationSeconds: null, provider: "gemini", model: "gemini-4-hydrogen",
        errorCode: null, workflowDescription, language: analysisLanguage,
      });
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setAnalysisBusy(false); }
  }
  async function sendAnalysisToGemini() {
    if (!activeAnalysis || activeAnalysis.status !== "awaiting_cloud_consent") return;
    const megabytes = (activeAnalysis.totalBytes / (1024 * 1024)).toFixed(2);
    const approved = window.confirm(
      "Send " + activeAnalysis.completedFrames + " sampled JPEG frames (" + megabytes + " MiB) and the optional workflow description to Google Gemini (" + activeAnalysis.model + ")?\n\nThe original MP4 will NOT be uploaded. The sampled frames may still contain private information; automatic redaction is not guaranteed. Only continue if you approve this transfer."
    );
    if (!approved) return;
    setAnalysisBusy(true); setError(null);
    try {
      await invoke("send_analysis_to_gemini", { runId: activeAnalysis.runId });
      setActiveAnalysis({ ...activeAnalysis, status: "sending_to_provider" });
      setNotice("Sampled frames sent to Gemini. Preparing your tutorial draft…");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setAnalysisBusy(false); }
  }
  async function cancelAnalysis() {
    if (!activeAnalysis) return;
    try {
      await invoke("cancel_analysis", { runId: activeAnalysis.runId });
      const next = await invoke<AnalysisStatus>("get_analysis_status", { runId: activeAnalysis.runId });
      setActiveAnalysis(next);
      setNotice("Analysis cancelled. The original recording was not changed.");
    } catch (reason) { setError(friendlyError(reason)); }
  }

  async function createEditableTimeline() {
    if (!activeAnalysis || activeAnalysis.status !== "completed") return;
    setTimelineBusy(true); setError(null);
    try {
      const created = await invoke<TimelineDocument>("create_timeline_from_draft", { runId: activeAnalysis.runId });
      setTimeline(created);
      setSavedTimelineSignature(JSON.stringify(created));
      setRenderStatus(null);
      setNotice("Editable timeline created at revision " + created.revision + ". The AI draft and original recording remain unchanged.");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setTimelineBusy(false); }
  }
  async function openTimeline(recording: Recording) {
    setTimelineBusy(true); setError(null);
    try {
      const opened = await invoke<TimelineDocument | null>("get_timeline_for_recording", { recordingId: recording.id });
      if (!opened) {
        setTimeline(null);
        setSavedTimelineSignature(null);
        setRenderStatus(null);
        setNotice("No editable timeline exists for this recording yet. Complete an AI analysis and choose Create editable timeline.");
      } else {
        setTimeline(opened);
        setSavedTimelineSignature(JSON.stringify(opened));
        const history = await invoke<RenderStatus[]>("list_render_history", { recordingId: opened.recordingId }).catch(() => []);
        setRenderStatus(history[0] ?? null);
        setNotice("Opened timeline · revision " + opened.revision + ".");
      }
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setTimelineBusy(false); }
  }
  function updateTimelineStep(index: number, patch: Partial<TimelineStep>) {
    setTimeline((current) => current ? {
      ...current,
      steps: current.steps.map((step, stepIndex) => stepIndex === index ? { ...step, ...patch } : step),
    } : current);
  }
  function addTimelineStep() {
    setTimeline((current) => {
      if (!current) return current;
      const lastEnd = current.steps.reduce((latest, step) => Math.max(latest, step.endMs), 0);
      if (lastEnd >= current.sourceDurationMs) {
        setError("There is no remaining time after the last step. Shorten a step before adding another.");
        return current;
      }
      const end = Math.min(current.sourceDurationMs, lastEnd + 3000);
      const id = "manual-" + Date.now();
      return {
        ...current,
        steps: [...current.steps, {
          id, title: "New tutorial step", description: "Describe what the viewer should do.",
          startMs: lastEnd, endMs: end, origin: "manual", evidenceFrameIds: [],
          confidence: null, evidenceKind: null,
        }],
      };
    });
  }
  function removeTimelineStep(index: number) {
    setTimeline((current) => {
      if (!current || current.steps.length <= 1) {
        setError("A timeline must contain at least one step.");
        return current;
      }
      return { ...current, steps: current.steps.filter((_, stepIndex) => stepIndex !== index) };
    });
  }
  function sortTimelineSteps() {
    setTimeline((current) => current ? { ...current, steps: [...current.steps].sort((a, b) => a.startMs - b.startMs) } : current);
  }
  function updateTimelineZoom(index: number, patch: Partial<TimelineZoom>) {
    setTimeline((current) => current ? {
      ...current,
      zoomSegments: current.zoomSegments.map((zoom, zoomIndex) => zoomIndex === index ? { ...zoom, ...patch } : zoom),
    } : current);
  }
  function addManualZoom() {
    setTimeline((current) => {
      if (!current) return current;
      const occupied = current.zoomSegments
        .filter((zoom) => zoom.origin === "manual" || zoom.approvalStatus === "accepted")
        .sort((a, b) => a.startMs - b.startMs);
      let start = 0;
      for (const zoom of occupied) {
        if (zoom.startMs - start >= 2000) break;
        start = Math.max(start, zoom.endMs);
      }
      if (current.sourceDurationMs - start < 500) {
        setError("There is no free time range for another active zoom. Move or reject an existing zoom first.");
        return current;
      }
      return {
        ...current,
        zoomSegments: [...current.zoomSegments, {
          id: "manual-zoom-" + Date.now(), startMs: start, endMs: Math.min(current.sourceDurationMs, start + 2000),
          x: 0.25, y: 0.25, width: 0.5, height: 0.5, targetScale: 1.5,
          rationale: "Manual focus region", evidenceFrameIds: [], origin: "manual",
          approvalStatus: "accepted", transitionDurationMs: 250,
        }],
      };
    });
  }
  function removeTimelineZoom(index: number) {
    setTimeline((current) => current ? {
      ...current, zoomSegments: current.zoomSegments.filter((_, zoomIndex) => zoomIndex !== index),
    } : current);
  }
  function addTimelineAnnotation() {
    setTimeline((current) => current ? {
      ...current,
      annotations: [...current.annotations, {
        id: "annotation-" + Date.now(), text: "Add your caption", language: "mixed",
        startMs: 0, endMs: Math.min(current.sourceDurationMs, 3000),
        x: 0.08, y: 0.08, maxWidth: 0.84, fontSize: 24,
        foreground: "#FFFFFF", background: "#000000CC", alignment: "center",
        direction: "auto", origin: "manual",
      }],
    } : current);
  }
  function updateTimelineAnnotation(index: number, patch: Partial<TimelineAnnotation>) {
    setTimeline((current) => current ? {
      ...current,
      annotations: current.annotations.map((annotation, annotationIndex) => annotationIndex === index ? { ...annotation, ...patch } : annotation),
    } : current);
  }
  function removeTimelineAnnotation(index: number) {
    setTimeline((current) => current ? {
      ...current, annotations: current.annotations.filter((_, annotationIndex) => annotationIndex !== index),
    } : current);
  }
  async function saveTimeline() {
    if (!timeline) return;
    setTimelineBusy(true); setError(null);
    try {
      const saved = await invoke<TimelineDocument>("save_timeline", { timeline, expectedRevision: timeline.revision });
      setTimeline(saved);
      setSavedTimelineSignature(JSON.stringify(saved));
      setNotice("Timeline saved · revision " + saved.revision + ". Previous revisions remain available.");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setTimelineBusy(false); }
  }
  async function reloadTimeline() {
    if (!timeline) return;
    setTimelineBusy(true); setError(null);
    try {
      const latest = await invoke<TimelineDocument | null>("get_timeline_for_recording", { recordingId: timeline.recordingId });
      if (latest) { setTimeline(latest); setSavedTimelineSignature(JSON.stringify(latest)); setRenderStatus(null); setNotice("Loaded the latest saved timeline revision."); }
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setTimelineBusy(false); }
  }

  async function startExport() {
    if (!timeline) return;
    if (timelineDirty) {
      setError("Save your timeline revision before exporting. Screenstory never exports unsaved edits.");
      return;
    }
    setRenderBusy(true); setError(null); setNotice(null);
    try {
      const started = await invoke<RenderStarted>("start_render", {
        recordingId: timeline.recordingId,
        expectedRevision: timeline.revision,
      });
      const initial = await invoke<RenderStatus>("get_render_status", { runId: started.runId });
      setRenderStatus(initial);
      setNotice("MP4 export started · timeline revision " + started.timelineRevision + ".");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setRenderBusy(false); }
  }
  async function cancelExport() {
    if (!renderStatus || ["completed", "failed", "cancelled", "interrupted", "missing"].includes(renderStatus.status)) return;
    setRenderBusy(true);
    try {
      await invoke("cancel_render", { runId: renderStatus.runId });
      const next = await invoke<RenderStatus>("get_render_status", { runId: renderStatus.runId });
      setRenderStatus(next);
      setNotice("Export cancellation requested. The source recording and saved timeline remain unchanged.");
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setRenderBusy(false); }
  }

  async function startRecording() {
    setBusy(true); setError(null); setNotice(null);
    try {
      const started = await invoke<{ fileName: string; startedAtMs: number }>("start_recording");
      setCapture({ isRecording: true, fileName: started.fileName, startedAtMs: started.startedAtMs });
      setClock(Date.now());
    } catch (reason) {
      setError(friendlyError(reason));
      const report = await invoke<DependencyReport>("check_dependencies").catch(() => null);
      if (report) setDependencies(report);
    } finally { setBusy(false); }
  }
  async function stopRecording() {
    setBusy(true); setError(null);
    try {
      const saved = await invoke<Recording>("stop_recording");
      setCapture({ isRecording: false, fileName: null, startedAtMs: null });
      setNotice("Saved " + saved.fileName + " · " + formatDuration(saved.durationSeconds));
      await refreshProjects();
    } catch (reason) {
      setError(friendlyError(reason));
      const status = await invoke<CaptureStatus>("get_recording_status").catch(() => null);
      if (status) setCapture(status);
    } finally { setBusy(false); }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <a className="brand" href="#" aria-label="Screenstory home"><span className="brand-mark">S</span><span>screenstory</span></a>
        <div className="topbar-right"><span className="topbar-label">LOCAL WORKSPACE</span><span className="version">{appInfo ? "v" + appInfo.version : "Starting…"}</span></div>
      </header>
      <div className="workspace">
        <aside className="sidebar">
          <div className="sidebar-heading"><span className="eyebrow">YOUR WORKSPACE</span><button className="icon-button" onClick={() => setShowCreate((value) => !value)} aria-label="Add project" disabled={busy || capture.isRecording || renderInProgress}>＋</button></div>
          <button className={"nav-item " + (!activeProject ? "selected" : "")} onClick={() => { if (!capture.isRecording) { setActiveProject(null); setRecordings([]); } }}>
            <span className="nav-icon">⌂</span> Overview
          </button>
          <div className="project-list-heading">PROJECTS <span>{projects.length}</span></div>
          {projects.length === 0 && <p className="sidebar-empty">Your projects will appear here.</p>}
          {projects.map((project) => (
            <button className={"project-nav " + (activeProject?.id === project.id ? "selected" : "")} key={project.id}
              onClick={() => void openProject(project)} disabled={busy || capture.isRecording || renderInProgress} title={project.name}>
              <span className="project-dot" /><span className="project-nav-name">{project.name}</span><span className="project-count">{project.recordingsCount}</span>
            </button>
          ))}
          <div className="sidebar-bottom"><span className="local-indicator" />Stored on this device</div>
        </aside>

        <section className="main-content">
          <div className="page-heading">
            <div><div className="eyebrow">SCREENSTORY STUDIO</div><h1>{activeProject ? activeProject.name : "Create a clear story."}</h1>
              <p>{activeProject ? "Record your workflow. Your original video stays safely in this project." : "Capture your workflow locally, then turn it into a tutorial."}</p></div>
            {!activeProject && <button className="primary-button" onClick={() => setShowCreate(true)}>＋ New project</button>}
          </div>
          {error && <div className="alert error" role="alert">{error}</div>}
          {notice && <div className="alert success" role="status">{notice}</div>}
          {showCreate && (
            <form className="create-card" onSubmit={createProject}>
              <div><span className="eyebrow">NEW PROJECT</span><h2>Name your workspace</h2></div>
              <input autoFocus value={projectName} onChange={(event) => setProjectName(event.target.value)} maxLength={80}
                placeholder="e.g. Customer onboarding" aria-label="Project name" />
              <div className="form-actions">
                <button type="button" className="secondary-button" onClick={() => setShowCreate(false)}>Cancel</button>
                <button type="submit" className="primary-button" disabled={busy || !projectName.trim()}>{busy ? "Creating…" : "Create project"}</button>
              </div>
            </form>
          )}

          {!activeProject ? (
            <div className="welcome-grid">
              <div className="hero-card">
                <div className="hero-copy"><span className="eyebrow">FROM WORKFLOW TO TUTORIAL</span>
                  <h2>Start with a recording.<br /><span>Build something useful.</span></h2>
                  <p>Create a project to keep recordings, analysis, and tutorial assets together in one local workspace.</p>
                  <button className="primary-button" onClick={() => setShowCreate(true)}>Create your first project <span>→</span></button>
                </div>
                <div className="hero-art" aria-hidden="true"><div className="screen-frame"><div className="screen-top"><i /><i /><i /></div>
                  <div className="screen-body"><div className="screen-side" /><div className="screen-lines"><i /><i /><i /><b /></div></div><div className="play-mark">▶</div></div></div>
              </div>
              <div className="feature-card"><div className="feature-icon">◉</div><h3>Capture locally</h3><p>Your original recording stays on your machine. No upload is required to record.</p></div>
              <div className="feature-card"><div className="feature-icon">▤</div><h3>Keep work organized</h3><p>Each project keeps its recordings and assets together in a dedicated folder.</p></div>
              <div className="feature-card"><div className="feature-icon">✦</div><h3>AI tutorial draft</h3><p>Review evidence-linked steps and convert them into a separate editable timeline after explicit cloud consent.</p></div>
            </div>
          ) : (
            <div className="project-view">
              <section className="analysis-settings">
                <div className="section-heading"><div><span className="eyebrow">AI PROVIDER</span><h2>Gemini analysis</h2></div>
                  <span className={geminiKeyConfigured ? "credential-pill ready" : "credential-pill"}>{geminiKeyConfigured ? "API KEY SAVED" : "SETUP REQUIRED"}</span></div>
                {!geminiKeyConfigured ? (
                  <div className="credential-form">
                    <p>Your key is stored in the Linux desktop credential store, not in the project or a plain-text file.</p>
                    {!credentialStoreAvailable && <p className="inline-warning">The desktop credential store could not be reached. Unlock your keyring and restart Screenstory.</p>}
                    <div className="credential-input-row">
                      <input type="password" autoComplete="off" value={geminiKeyInput} onChange={(event) => setGeminiKeyInput(event.target.value)} placeholder="Paste your Gemini API key" aria-label="Gemini API key" />
                      <button className="secondary-button" onClick={() => void saveGeminiKey()} disabled={analysisBusy || geminiKeyInput.trim().length < 20}>Save key</button>
                    </div>
                    <small>Create a key in Google AI Studio. Screenstory does not send anything until you approve the analysis request.</small>
                  </div>
                ) : (
                  <div className="credential-form saved-credential">
                    <p>Your Gemini key is stored securely in the desktop credential store.</p>
                    <button className="text-button" onClick={() => void deleteGeminiKey()} disabled={analysisBusy}>Remove API key</button>
                  </div>
                )}
                <label className="field-label" htmlFor="workflow-description">Optional workflow context</label>
                <textarea id="workflow-description" value={workflowDescription} maxLength={2000} onChange={(event) => setWorkflowDescription(event.target.value)} placeholder="Describe what this workflow is meant to accomplish. This is context for Gemini, not proof of what happened." />
                <div className="language-row"><label className="field-label" htmlFor="analysis-language">Tutorial language</label>
                  <select id="analysis-language" value={analysisLanguage} onChange={(event) => setAnalysisLanguage(event.target.value as "auto" | "en" | "ar")}>
                    <option value="auto">Automatic</option><option value="en">English</option><option value="ar">العربية</option>
                  </select>
                </div>
              </section>

              {activeAnalysis && (
                <section className="analysis-progress-card">
                  <div className="section-heading"><div><span className="eyebrow">ANALYSIS RUN</span><h2>{activeAnalysis.sourceFileName}</h2></div><span className={"analysis-status-pill " + activeAnalysis.status}>{activeAnalysis.status.replaceAll("_", " ")}</span></div>
                  {activeAnalysis.status === "extracting_frames" && <>
                    <p>Preparing representative frames locally. The original MP4 stays on this device.</p>
                    <div className="progress-track"><span style={{ width: activeAnalysis.totalFrames ? (activeAnalysis.completedFrames / activeAnalysis.totalFrames * 100) + "%" : "4%" }} /></div>
                    <small>{activeAnalysis.completedFrames} / {activeAnalysis.totalFrames || "…"} frames extracted</small>
                  </>}
                  {activeAnalysis.status === "awaiting_cloud_consent" && <>
                    <p>Local frame preparation is complete. No data has been sent yet.</p>
                    <div className="consent-details"><strong>Transfer review</strong><span>Provider: Google Gemini · {activeAnalysis.model}</span><span>Recording: {activeAnalysis.sourceFileName} · {formatDuration(activeAnalysis.durationSeconds ?? 0)}</span><span>Sampled frames: {activeAnalysis.completedFrames} · {(activeAnalysis.totalBytes / (1024 * 1024)).toFixed(2)} MiB</span><span>Workflow text: {activeAnalysis.workflowDescription.trim() ? "Included" : "None"}</span><b>Only sampled frames and the optional text are sent. The original MP4 is not uploaded.</b><small>Frames can still contain private information. Automatic redaction is not guaranteed.</small></div>
                    <div className="analysis-actions"><button className="primary-button" onClick={() => void sendAnalysisToGemini()} disabled={!geminiKeyConfigured || analysisBusy}>Approve & send frames to Gemini</button><button className="secondary-button" onClick={() => void cancelAnalysis()}>Cancel run</button></div>
                    {!geminiKeyConfigured && <small>Save a Gemini API key above before approving the transfer.</small>}
                  </>}
                  {activeAnalysis.status === "sending_to_provider" && <>
                    <p>Gemini is reviewing the approved sample frames and preparing evidence-linked tutorial steps.</p>
                    <div className="progress-track"><span className="indeterminate" /></div>
                    <button className="secondary-button" onClick={() => void cancelAnalysis()}>Cancel analysis</button>
                  </>}
                  {activeAnalysis.status === "completed" && analysisDraft && <>
                    <div className="draft-summary"><strong>AI draft ready for review</strong><p>{analysisDraft.summary}</p><div className="draft-counts"><span>{analysisDraft.steps.length} tutorial steps</span><span>{analysisDraft.zoomSuggestions.length} zoom suggestions</span></div></div>
                    <div className="draft-step-list">{analysisDraft.steps.map((step, index) => <article className="draft-step" key={step.id}><span className="draft-index">{String(index + 1).padStart(2, "0")}</span><div><strong>{step.title}</strong><p>{step.description}</p><small>{formatDuration(step.startMs / 1000)} – {formatDuration(step.endMs / 1000)} · {step.evidenceFrameIds.length} evidence frames · {step.evidenceKind}</small></div></article>)}</div>
                    <div className="analysis-actions"><button className="primary-button" onClick={() => void createEditableTimeline()} disabled={timelineBusy || timeline !== null}>Create editable timeline</button><small>Creates a separate versioned timeline. The AI draft and source video are untouched.</small></div>
                    <small>This draft is not approved for rendering and does not overwrite your original recording.</small>
                  </>}
                  {activeAnalysis.status === "failed" && <div className="inline-warning">Analysis failed ({activeAnalysis.errorCode ?? "unknown error"}). Your original recording is unchanged. You can start another run.</div>}
                  {activeAnalysis.status === "cancelled" && <p>Analysis cancelled. Your original recording is unchanged.</p>}
                </section>
              )}

              {timeline && (
                <section className="timeline-editor-card">
                  <div className="section-heading"><div><span className="eyebrow">VERSIONED TIMELINE</span><h2>Edit tutorial steps</h2><p>{timeline.sourceFileName} · {timeline.steps.length} steps · revision {timeline.revision}</p></div><span className="analysis-status-pill completed">LOCAL DRAFT</span></div>
                  <div className="timeline-toolbar">
                    <button className="secondary-button" onClick={() => void reloadTimeline()} disabled={timelineBusy}>Reload latest</button>
                    <button className="secondary-button" onClick={sortTimelineSteps} disabled={timelineBusy}>Sort by start time</button>
                    <button className="primary-button" onClick={() => void saveTimeline()} disabled={timelineBusy}>{timelineBusy ? "Saving…" : "Save revision"}</button>
                    <button className="primary-button export-button" onClick={() => void startExport()} disabled={timelineBusy || renderBusy || timelineDirty || capture.isRecording || (!!renderStatus && !["completed", "failed", "cancelled", "interrupted", "missing"].includes(renderStatus.status))}>{renderBusy ? "Preparing…" : "Export MP4"}</button>
                  </div>
                  <div className="timeline-step-list">
                    {timeline.steps.map((step, index) => (
                      <article className="timeline-step-card" key={step.id}>
                        <div className="timeline-step-top"><span className="draft-index">{String(index + 1).padStart(2, "0")}</span><span className={"timeline-origin " + step.origin}>{step.origin === "ai" ? "AI SUGGESTED" : "MANUAL"}</span><button className="text-button danger-text" onClick={() => removeTimelineStep(index)} disabled={timelineBusy || timeline.steps.length <= 1}>Remove</button></div>
                        <label className="field-label" htmlFor={"timeline-title-" + step.id}>Step title</label>
                        <input id={"timeline-title-" + step.id} className="timeline-text-input" value={step.title} maxLength={160} onChange={(event) => updateTimelineStep(index, { title: event.target.value })} />
                        <label className="field-label" htmlFor={"timeline-description-" + step.id}>Description</label>
                        <textarea id={"timeline-description-" + step.id} className="timeline-textarea" value={step.description} maxLength={1200} onChange={(event) => updateTimelineStep(index, { description: event.target.value })} />
                        <div className="timeline-time-grid">
                          <label>Start (ms)<input type="number" min={0} max={timeline.sourceDurationMs} step={100} value={step.startMs} onChange={(event) => updateTimelineStep(index, { startMs: Number(event.target.value) })} /></label>
                          <label>End (ms)<input type="number" min={0} max={timeline.sourceDurationMs} step={100} value={step.endMs} onChange={(event) => updateTimelineStep(index, { endMs: Number(event.target.value) })} /></label>
                          <span>{formatDuration(step.startMs / 1000)} – {formatDuration(step.endMs / 1000)}</span>
                        </div>
                        <div className="timeline-evidence">{step.origin === "ai" ? (step.evidenceFrameIds.length + " evidence frames · confidence " + (step.confidence ?? 0).toFixed(2)) : "User-authored step · no AI evidence"}</div>
                      </article>
                    ))}
                  </div>
                  <div className="timeline-footer"><button className="secondary-button" onClick={addTimelineStep} disabled={timelineBusy || timeline.steps.length >= 60}>＋ Add manual step</button><span>Changes are local until you save a new revision.</span></div>
                  <section className="timeline-tools-section">
                    <div className="section-heading"><div><span className="eyebrow">CAMERA MOVEMENT</span><h3>Zoom suggestions</h3></div><button className="secondary-button compact" onClick={addManualZoom} disabled={timelineBusy}>＋ Manual zoom</button></div>
                    {timeline.zoomSegments.length === 0 && <p className="timeline-muted">No AI zoom suggestions were generated. You can add a manual zoom focus.</p>}
                    {timeline.zoomSegments.map((zoom, index) => (
                      <article className="zoom-editor-card" key={zoom.id}>
                        <div className="timeline-step-top"><strong>{zoom.origin === "ai" ? "AI zoom suggestion" : "Manual zoom"}</strong><span className={"zoom-state " + zoom.approvalStatus}>{zoom.approvalStatus.toUpperCase()}</span><button className="text-button danger-text" onClick={() => removeTimelineZoom(index)} disabled={timelineBusy}>Remove</button></div>
                        {zoom.origin === "ai" && <p className="timeline-muted">{zoom.rationale} · {zoom.evidenceFrameIds.length} evidence frames</p>}
                        <div className="timeline-time-grid">
                          <label>Start ms<input type="number" min={0} max={timeline.sourceDurationMs} value={zoom.startMs} onChange={(event) => updateTimelineZoom(index, { startMs: Number(event.target.value) })} /></label>
                          <label>End ms<input type="number" min={0} max={timeline.sourceDurationMs} value={zoom.endMs} onChange={(event) => updateTimelineZoom(index, { endMs: Number(event.target.value) })} /></label>
                          <label>Scale<input type="number" min={1} max={3} step={0.1} value={zoom.targetScale} onChange={(event) => updateTimelineZoom(index, { targetScale: Number(event.target.value) })} /></label>
                          <label>Transition ms<input type="number" min={0} max={1500} step={50} value={zoom.transitionDurationMs} onChange={(event) => updateTimelineZoom(index, { transitionDurationMs: Number(event.target.value) })} /></label>
                        </div>
                        <div className="timeline-time-grid zoom-geometry">
                          <label>Region X<input type="number" min={0} max={1} step={0.01} value={zoom.x} onChange={(event) => updateTimelineZoom(index, { x: Number(event.target.value) })} /></label>
                          <label>Region Y<input type="number" min={0} max={1} step={0.01} value={zoom.y} onChange={(event) => updateTimelineZoom(index, { y: Number(event.target.value) })} /></label>
                          <label>Width<input type="number" min={0.01} max={1} step={0.01} value={zoom.width} onChange={(event) => updateTimelineZoom(index, { width: Number(event.target.value) })} /></label>
                          <label>Height<input type="number" min={0.01} max={1} step={0.01} value={zoom.height} onChange={(event) => updateTimelineZoom(index, { height: Number(event.target.value) })} /></label>
                        </div>
                        {zoom.origin === "ai" && <div className="analysis-actions">
                          <button className="secondary-button compact" onClick={() => updateTimelineZoom(index, { approvalStatus: "accepted" })} disabled={timelineBusy || zoom.approvalStatus === "accepted"}>Accept zoom</button>
                          <button className="secondary-button compact" onClick={() => updateTimelineZoom(index, { approvalStatus: "rejected" })} disabled={timelineBusy || zoom.approvalStatus === "rejected"}>Reject suggestion</button>
                          {zoom.approvalStatus !== "pending" && <button className="text-button" onClick={() => updateTimelineZoom(index, { approvalStatus: "pending" })} disabled={timelineBusy}>Reset to pending</button>}
                        </div>}
                      </article>
                    ))}
                  </section>

                  <section className="timeline-tools-section">
                    <div className="section-heading"><div><span className="eyebrow">ON-SCREEN TEXT</span><h3>Annotations</h3></div><button className="secondary-button compact" onClick={addTimelineAnnotation} disabled={timelineBusy}>＋ Add caption</button></div>
                    {timeline.annotations.length === 0 && <p className="timeline-muted">No captions yet. Add Arabic, English, or mixed-language text overlays.</p>}
                    {timeline.annotations.map((annotation, index) => (
                      <article className="zoom-editor-card" key={annotation.id}>
                        <div className="timeline-step-top"><strong>Caption {index + 1}</strong><span className="timeline-origin manual">MANUAL</span><button className="text-button danger-text" onClick={() => removeTimelineAnnotation(index)} disabled={timelineBusy}>Remove</button></div>
                        <label className="field-label" htmlFor={"annotation-text-" + annotation.id}>Caption text</label>
                        <textarea id={"annotation-text-" + annotation.id} className="timeline-textarea" maxLength={500} value={annotation.text} onChange={(event) => updateTimelineAnnotation(index, { text: event.target.value })} />
                        <div className="timeline-time-grid">
                          <label>Start ms<input type="number" min={0} max={timeline.sourceDurationMs} value={annotation.startMs} onChange={(event) => updateTimelineAnnotation(index, { startMs: Number(event.target.value) })} /></label>
                          <label>End ms<input type="number" min={0} max={timeline.sourceDurationMs} value={annotation.endMs} onChange={(event) => updateTimelineAnnotation(index, { endMs: Number(event.target.value) })} /></label>
                          <label>Font px<input type="number" min={12} max={72} value={annotation.fontSize} onChange={(event) => updateTimelineAnnotation(index, { fontSize: Number(event.target.value) })} /></label>
                        </div>
                        <div className="annotation-options-grid">
                          <label>Language<select value={annotation.language} onChange={(event) => updateTimelineAnnotation(index, { language: event.target.value as TimelineAnnotation["language"] })}><option value="ar">Arabic</option><option value="en">English</option><option value="mixed">Mixed</option></select></label>
                          <label>Direction<select value={annotation.direction} onChange={(event) => updateTimelineAnnotation(index, { direction: event.target.value as TimelineAnnotation["direction"] })}><option value="auto">Automatic</option><option value="rtl">RTL</option><option value="ltr">LTR</option></select></label>
                          <label>Alignment<select value={annotation.alignment} onChange={(event) => updateTimelineAnnotation(index, { alignment: event.target.value as TimelineAnnotation["alignment"] })}><option value="right">Right</option><option value="center">Center</option><option value="left">Left</option></select></label>
                        </div>
                        <div className="timeline-time-grid">
                          <label>X<input type="number" min={0} max={1} step={0.01} value={annotation.x} onChange={(event) => updateTimelineAnnotation(index, { x: Number(event.target.value) })} /></label>
                          <label>Y<input type="number" min={0} max={1} step={0.01} value={annotation.y} onChange={(event) => updateTimelineAnnotation(index, { y: Number(event.target.value) })} /></label>
                          <label>Max width<input type="number" min={0.1} max={1} step={0.01} value={annotation.maxWidth} onChange={(event) => updateTimelineAnnotation(index, { maxWidth: Number(event.target.value) })} /></label>
                        </div>
                        <div className="annotation-options-grid">
                          <label>Text color<input type="text" value={annotation.foreground} onChange={(event) => updateTimelineAnnotation(index, { foreground: event.target.value })} /></label>
                          <label>Background color<input type="text" value={annotation.background} onChange={(event) => updateTimelineAnnotation(index, { background: event.target.value })} /></label>
                        </div>
                      </article>
                    ))}
                    {timelineDirty && <div className="pending-zoom-note">You have unsaved timeline edits. Save a new revision before exporting; the source recording is never changed.</div>}
                    {renderStatus && (
                      <section className="render-progress-card" aria-live="polite">
                        <div className="section-heading">
                          <div><span className="eyebrow">MP4 EXPORT · REVISION {renderStatus.timelineRevision}</span><h3>{renderStatus.outputFileName}</h3></div>
                          <span className={"analysis-status-pill " + renderStatus.status}>{renderStatus.status.replaceAll("_", " ")}</span>
                        </div>
                        <div className="progress-track"><span style={{ width: renderStatus.progress + "%" }} /></div>
                        <small>{renderStatus.status === "completed" ? "Export verified · " + (renderStatus.outputSizeBytes / (1024 * 1024)).toFixed(1) + " MiB" : renderStatus.status === "failed" ? "Export failed · " + (renderStatus.errorCode ?? "unknown error") : renderStatus.status === "cancelled" ? "Export cancelled. No partial MP4 was kept." : renderStatus.status === "interrupted" ? "Screenstory closed during export. The partial MP4 was removed; your source and timeline are safe." : renderStatus.status === "missing" ? "The export file is no longer present. You can render this saved revision again." : renderStatus.progress + "% · Processing source video"}</small>
                        {renderStatus.status === "completed" && renderStatus.outputPath && <div className="export-result"><strong>Your tutorial is ready</strong><p>{renderStatus.outputPath}</p><span>{renderStatus.activeZoomCount} zoom effects · {renderStatus.annotationCount} captions · {renderStatus.hasAudio ? "source audio preserved" : "no source audio"}</span>
                          <video className="tutorial-preview" controls preload="metadata" src={convertFileSrc(renderStatus.outputPath)} />
                        </div>}
                        {!["completed", "failed", "cancelled", "interrupted", "missing"].includes(renderStatus.status) && <div className="analysis-actions"><button className="secondary-button compact" onClick={() => void cancelExport()} disabled={renderBusy}>Cancel export</button></div>}
                      </section>
                    )}

                  </section>
                </section>
              )}

              <section className="record-card">
                <div className="record-card-heading"><div><span className="eyebrow">RECORDING STUDIO</span><h2>Capture your workflow</h2><p>Full desktop · X11 · 30 FPS · MP4</p></div>
                  <span className={"recording-state " + (capture.isRecording ? "live" : "")}><i />{capture.isRecording ? "RECORDING" : "READY"}</span></div>
                <div className="capture-preview"><div className="capture-window"><div className="capture-window-top"><span /><span /><span /></div>
                  <div className="capture-center"><span className={"capture-pulse " + (capture.isRecording ? "active" : "")} />
                    <strong>{capture.isRecording ? formatDuration(elapsedSeconds) : "Ready when you are"}</strong>
                    <small>{capture.isRecording ? capture.fileName : "Your entire desktop will be captured."}</small></div></div></div>
                <div className="record-controls">
                  {capture.isRecording ? <button className="stop-button" onClick={() => void stopRecording()} disabled={busy}><span className="stop-square" />{busy ? "Finalizing MP4…" : "Stop & save"}</button>
                    : <button className="record-button" onClick={() => void startRecording()} disabled={busy || loading || ffmpeg?.status !== "available"}><span className="record-circle" />{busy ? "Starting…" : "Start recording"}</button>}
                  <span className="record-hint">{capture.isRecording ? "Keep Screenstory open until the MP4 is finalized." : "Recording begins after you press Start. Nothing is captured before then."}</span>
                </div>
                {ffmpeg?.status !== "available" && <div className="dependency-warning"><strong>FFmpeg is required to record.</strong>
                  <p>{ffmpeg?.remediation ?? "Checking FFmpeg availability…"}</p>
                  <button className="secondary-button" onClick={() => void invoke<DependencyReport>("check_dependencies").then(setDependencies)}>Recheck dependency</button>
                </div>}
              </section>
              <section className="recordings-card"><div className="section-heading"><div><span className="eyebrow">PROJECT LIBRARY</span><h2>Recordings</h2></div><span className="count-pill">{recordings.length}</span></div>
                {recordings.length === 0 ? <div className="recordings-empty"><span className="empty-video-icon">▹</span><strong>No recordings yet</strong><p>Your saved desktop recordings will appear here.</p></div>
                  : <div className="recording-list">{recordings.map((recording) => <div className="recording-row" key={recording.id}>
                    <div className="recording-file-icon">MP4</div><div className="recording-details"><strong>{recording.fileName}</strong><span>{formatDate(recording.createdAt)} · {formatDuration(recording.durationSeconds)}</span></div><button className="secondary-button compact" onClick={() => void openTimeline(recording)} disabled={timelineBusy}>Open timeline</button><button className="analysis-button" onClick={() => void startAnalysis(recording)} disabled={analysisBusy || capture.isRecording || !ffmpeg || ffmpeg.status !== "available" || (activeAnalysis !== null && !["completed", "failed", "cancelled", "interrupted", "missing"].includes(activeAnalysis.status))}>Analyze</button><span className="saved-label">SAVED</span>
                  </div>)}</div>}
              </section>
            </div>
          )}
          {loading && <div className="loading-line">Loading your local workspace…</div>}
        </section>
      </div>
      <footer className="statusbar"><span><i className="status-dot ready" />{appInfo ? "Native core connected" : "Connecting to native core…"}</span>
        <span>{activeProject ? "Project · " + activeProject.recordingsCount + " recordings" : "Local-first · Your files stay on this device"}</span></footer>
    </main>
  );
}
