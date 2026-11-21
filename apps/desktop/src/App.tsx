import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type AppInfo = { name: string; version: string; runtime: string };
type Dependency = {
  id: string; feature: string; status: "available" | "missing" | "unsupported" | "error";
  version: string | null; impact: string; canInstall: boolean; remediation: string | null;
};
type DependencyReport = { dependencies: Dependency[] };
type InstallResult = { success: boolean; message: string; report: DependencyReport };

export default function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [bridgeError, setBridgeError] = useState<string | null>(null);
  const [report, setReport] = useState<DependencyReport | null>(null);
  const [checking, setChecking] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [installMessage, setInstallMessage] = useState<string | null>(null);

  async function refreshDependencies() {
    setChecking(true);
    try {
      setReport(await invoke<DependencyReport>("check_dependencies"));
    } catch {
      setInstallMessage("Could not check feature dependencies. The desktop shell remains available.");
    } finally {
      setChecking(false);
    }
  }

  useEffect(() => {
    let active = true;
    invoke<AppInfo>("get_app_info")
      .then((info) => { if (active) setAppInfo(info); })
      .catch(() => { if (active) setBridgeError("The native application bridge is unavailable."); });
    invoke<DependencyReport>("check_dependencies")
      .then((result) => { if (active) setReport(result); })
      .catch(() => { if (active) setInstallMessage("Could not check feature dependencies. The desktop shell remains available."); })
      .finally(() => { if (active) setChecking(false); });
    return () => { active = false; };
  }, []);

  async function installFfmpeg() {
    const accepted = window.confirm(
      "Screenstory needs FFmpeg for recording and video processing.\n\n" +
      "This will ask your system to authorize installing the 'ffmpeg' package from your configured Linux software repositories. " +
      "No third-party repository will be added. Continue?"
    );
    if (!accepted) return;
    setInstalling(true);
    setInstallMessage("Waiting for system authorization and package installation…");
    try {
      const result = await invoke<InstallResult>("install_ffmpeg");
      setReport(result.report);
      setInstallMessage(result.message);
    } catch {
      setInstallMessage("Installation could not be completed. Other parts of Screenstory remain available.");
      await refreshDependencies();
    } finally {
      setInstalling(false);
    }
  }

  const ffmpeg = report?.dependencies.find((item) => item.id === "ffmpeg");

  return (
    <main className="app-shell">
      <header className="topbar">
        <a className="brand" href="#" aria-label="Screenstory home">
          <span className="brand-mark">S</span><span>screenstory</span>
        </a>
        <span className="topbar-label">DESKTOP WORKSPACE</span>
      </header>
      <section className="welcome">
        <div className="eyebrow">YOUR WORKFLOW, MADE CLEAR</div>
        <h1>Turn screen recordings<br />into <span>clear tutorials.</span></h1>
        <p className="intro">Record a workflow, shape its story, and create a tutorial people can follow.</p>
        <div className="empty-project">
          <div className="empty-icon" aria-hidden="true">＋</div>
          <h2>Your workspace is ready</h2>
          <p>No project is open yet. Project creation will arrive with the project-management milestone.</p>
          <button type="button" disabled title="Project creation is not part of the desktop foundation milestone">
            New project <span>Coming next</span>
          </button>
        </div>
        <section className="dependency-panel" aria-labelledby="dependency-heading">
          <div className="dependency-heading">
            <div><div className="eyebrow">SYSTEM READINESS</div><h2 id="dependency-heading">Feature requirements</h2></div>
            <button className="refresh-button" onClick={refreshDependencies} disabled={checking || installing}>Recheck</button>
          </div>
          {checking && <p className="dependency-copy">Checking available features…</p>}
          {ffmpeg && (
            <div className="dependency-row">
              <div className="dependency-main">
                <span className={ffmpeg.status === "available" ? "status-dot ready" : "status-dot"} />
                <div><strong>FFmpeg</strong><p>{ffmpeg.status === "available" ? ffmpeg.version : ffmpeg.impact}</p></div>
              </div>
              {ffmpeg.status !== "available" && ffmpeg.canInstall && (
                <button className="install-button" onClick={installFfmpeg} disabled={installing}>
                  {installing ? "Installing…" : "Install FFmpeg"}
                </button>
              )}
              {ffmpeg.status !== "available" && !ffmpeg.canInstall && <span className="dependency-note">Manual installation required</span>}
            </div>
          )}
          {installMessage && <p className="dependency-message" role="status">{installMessage}</p>}
          <p className="dependency-footnote">Missing video tools do not prevent the desktop workspace from opening.</p>
        </section>
      </section>
      <footer className="statusbar">
        <span><i className={appInfo ? "status-dot ready" : "status-dot"} /> {appInfo ? "Native core connected" : bridgeError ?? "Connecting to native core…"}</span>
        <span>{appInfo ? `v${appInfo.version} · ${appInfo.runtime}` : "Desktop foundation preview"}</span>
      </footer>
    </main>
  );
}
