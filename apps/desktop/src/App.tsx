import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type AppInfo = {
  name: string;
  version: string;
  runtime: string;
};

export default function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [bridgeError, setBridgeError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    invoke<AppInfo>("get_app_info")
      .then((info) => {
        if (active) setAppInfo(info);
      })
      .catch(() => {
        if (active) setBridgeError("The native application bridge is unavailable.");
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="app-shell">
      <header className="topbar">
        <a className="brand" href="#" aria-label="Screenstory home">
          <span className="brand-mark">S</span>
          <span>screenstory</span>
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
      </section>
      <footer className="statusbar">
        <span><i className={appInfo ? "status-dot ready" : "status-dot"} /> {appInfo ? "Native core connected" : bridgeError ?? "Connecting to native core…"}</span>
        <span>{appInfo ? `v${appInfo.version} · ${appInfo.runtime}` : "Desktop foundation preview"}</span>
      </footer>
    </main>
  );
}