import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";

type AppInfo = { name: string; version: string; runtime: string };
type Dependency = { id: string; feature: string; status: "available" | "missing" | "unsupported" | "error"; version: string | null; impact: string; canInstall: boolean; remediation: string | null };
type DependencyReport = { dependencies: Dependency[] };
type Project = { id: string; name: string; createdAt: string; updatedAt: string; recordingsCount: number };
type Recording = { id: string; fileName: string; createdAt: string; durationSeconds: number };
type CaptureStatus = { isRecording: boolean; fileName: string | null; startedAtMs: number | null };

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

  const ffmpeg = dependencies?.dependencies.find((item) => item.id === "ffmpeg");
  const elapsedSeconds = useMemo(() => capture.startedAtMs === null ? 0 : Math.floor((clock - capture.startedAtMs) / 1000), [capture.startedAtMs, clock]);

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
      await refreshProjects();
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => { void bootstrap(); }, []);
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
      setActiveProject(opened); setNotice("Opened “" + opened.name + "”.");
      await refreshProjects();
    } catch (reason) { setError(friendlyError(reason)); }
    finally { setBusy(false); }
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
          <div className="sidebar-heading"><span className="eyebrow">YOUR WORKSPACE</span><button className="icon-button" onClick={() => setShowCreate((value) => !value)} aria-label="Create project">＋</button></div>
          <button className={"nav-item " + (!activeProject ? "selected" : "")} onClick={() => { if (!capture.isRecording) { setActiveProject(null); setRecordings([]); } }}>
            <span className="nav-icon">⌂</span> Overview
          </button>
          <div className="project-list-heading">PROJECTS <span>{projects.length}</span></div>
          {projects.length === 0 && <p className="sidebar-empty">Your projects will appear here.</p>}
          {projects.map((project) => (
            <button className={"project-nav " + (activeProject?.id === project.id ? "selected" : "")} key={project.id}
              onClick={() => void openProject(project)} disabled={busy || capture.isRecording} title={project.name}>
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
              <div className="feature-card muted-feature"><div className="feature-icon">✦</div><h3>AI tutorial analysis</h3><p>Planned next: turn recorded steps into an editable tutorial timeline.</p><span className="coming-label">NOT IN THIS BUILD</span></div>
            </div>
          ) : (
            <div className="project-view">
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
                    <div className="recording-file-icon">MP4</div><div className="recording-details"><strong>{recording.fileName}</strong><span>{formatDate(recording.createdAt)} · {formatDuration(recording.durationSeconds)}</span></div><span className="saved-label">SAVED</span>
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
