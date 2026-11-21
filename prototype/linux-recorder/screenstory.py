#!/usr/bin/env python3
"""Screenstory first-day Linux X11 recorder prototype.

Scope: full-desktop recording to a local MP4 using the host FFmpeg build.
This is a disposable product slice, not the final Tauri/Rust architecture.
"""
from __future__ import annotations

import datetime as dt
import os
from pathlib import Path
import re
import shutil
import subprocess
import threading
import tkinter as tk
from tkinter import filedialog, messagebox, ttk


APP_NAME = "Screenstory"
BASE_PACKAGES = ["ffmpeg"]
WINDOW_PACKAGES = ["wmctrl"]
DEFAULT_OUTPUT = Path.home() / "Videos" / "Screenstory"


def screen_size() -> tuple[int, int]:
    """Read the current X11 screen size from xrandr, with xdpyinfo fallback."""
    if shutil.which("xrandr"):
        result = subprocess.run(
            ["xrandr", "--current"], capture_output=True, text=True, check=False
        )
        match = re.search(r"current\s+(\d+)\s+x\s+(\d+)", result.stdout)
        if result.returncode == 0 and match:
            return int(match.group(1)), int(match.group(2))

    if shutil.which("xdpyinfo"):
        result = subprocess.run(
            ["xdpyinfo"], capture_output=True, text=True, check=False
        )
        match = re.search(r"dimensions:\s+(\d+)x(\d+) pixels", result.stdout)
        if result.returncode == 0 and match:
            return int(match.group(1)), int(match.group(2))

    raise RuntimeError("Could not detect screen resolution (xrandr/xdpyinfo unavailable).")


class ScreenstoryRecorder:
    def __init__(self, root: tk.Tk) -> None:
        self.root = root
        self.root.title("Screenstory")
        self.root.geometry("560x360")
        self.root.minsize(500, 330)
        self.root.protocol("WM_DELETE_WINDOW", self.on_close)

        self.process: subprocess.Popen | None = None
        self.capture_process: subprocess.Popen | None = None
        self.window_choices: dict[str, tuple[str, int, int]] = {}
        self.log_handle = None
        self.output_file: Path | None = None
        self.started_at: float | None = None
        self.closing = False

        DEFAULT_OUTPUT.mkdir(parents=True, exist_ok=True)
        self.output_dir = tk.StringVar(value=str(DEFAULT_OUTPUT))
        self.status = tk.StringVar(value="Ready to record")
        self.elapsed = tk.StringVar(value="00:00:00")
        self.source_info = tk.StringVar(value=self.describe_source())
        self.source_mode = tk.StringVar(value="desktop")
        self.selected_window = tk.StringVar(value="")

        self.build_ui()
        self.check_readiness()
        self.refresh_windows()
        self.update_source_controls()

    def describe_source(self) -> str:
        session = os.environ.get("XDG_SESSION_TYPE", "unknown").lower()
        display = os.environ.get("DISPLAY", "")
        if session != "x11" or not display:
            return f"Unsupported session: {session or 'unknown'}"
        try:
            width, height = screen_size()
            return f"Full desktop · {width} × {height} · X11"
        except Exception as exc:
            return f"X11 detected · {exc}"

    def build_ui(self) -> None:
        outer = ttk.Frame(self.root, padding=22)
        outer.pack(fill="both", expand=True)

        ttk.Label(outer, text="Screenstory", font=("TkDefaultFont", 22, "bold")).pack(anchor="w")
        ttk.Label(
            outer,
            text="Turn your workflow into a tutorial.",
            foreground="#666666",
        ).pack(anchor="w", pady=(2, 18))

        source = ttk.LabelFrame(outer, text="Recording source", padding=12)
        source.pack(fill="x")
        ttk.Label(source, textvariable=self.source_info).pack(anchor="w")
        source_row = ttk.Frame(source)
        source_row.pack(fill="x", pady=(8, 0))
        ttk.Radiobutton(source_row, text="Full desktop", variable=self.source_mode,
                        value="desktop", command=self.update_source_controls).pack(side="left")
        ttk.Radiobutton(source_row, text="A window", variable=self.source_mode,
                        value="window", command=self.update_source_controls).pack(side="left", padx=(12, 0))
        self.window_combo = ttk.Combobox(source, textvariable=self.selected_window,
                                         state="readonly", width=48)
        self.window_combo.pack(fill="x", pady=(7, 0))
        window_actions = ttk.Frame(source)
        window_actions.pack(fill="x", pady=(5, 0))
        self.refresh_windows_button = ttk.Button(window_actions, text="Refresh windows",
                                                  command=self.refresh_windows)
        self.refresh_windows_button.pack(side="left")
        self.window_hint = ttk.Label(window_actions, text="X11 window capture via FFmpeg")
        self.window_hint.pack(side="left", padx=(10, 0))

        destination = ttk.LabelFrame(outer, text="Save recordings to", padding=12)
        destination.pack(fill="x", pady=12)
        row = ttk.Frame(destination)
        row.pack(fill="x")
        ttk.Entry(row, textvariable=self.output_dir).pack(side="left", fill="x", expand=True)
        ttk.Button(row, text="Browse…", command=self.choose_folder).pack(side="left", padx=(8, 0))

        controls = ttk.Frame(outer)
        controls.pack(fill="x", pady=(8, 4))
        self.record_button = ttk.Button(controls, text="●  Start recording", command=self.start_recording)
        self.record_button.pack(side="left", fill="x", expand=True, ipady=9)
        self.stop_button = ttk.Button(
            controls, text="■  Stop", command=self.stop_recording, state="disabled"
        )
        self.stop_button.pack(side="left", padx=(10, 0), ipadx=18, ipady=9)

        ttk.Label(outer, textvariable=self.elapsed, font=("TkDefaultFont", 24, "bold")).pack(
            anchor="center", pady=(12, 2)
        )
        ttk.Label(outer, textvariable=self.status).pack(anchor="center")
        ttk.Label(
            outer,
            text="Your recording stays on this computer. No AI or network connection is used.",
            foreground="#666666",
            wraplength=500,
        ).pack(anchor="center", pady=(18, 0))

    def update_source_controls(self) -> None:
        window_mode = self.source_mode.get() == "window"
        self.window_combo.configure(state="readonly" if window_mode else "disabled")
        self.refresh_windows_button.configure(state="normal" if window_mode else "disabled")

    def refresh_windows(self) -> None:
        self.window_choices.clear()
        if not shutil.which("wmctrl"):
            self.window_combo["values"] = []
            self.window_hint.configure(text="Install wmctrl to list windows")
            return
        result = subprocess.run(["wmctrl", "-lG"], capture_output=True, text=True, check=False)
        labels = []
        for line in result.stdout.splitlines():
            parts = line.split(maxsplit=7)
            if len(parts) < 7:
                continue
            wid, _desktop, _x, _y, width, height, host = parts[:7]
            title = parts[7] if len(parts) > 7 else "(untitled)"
            if int(width) < 80 or int(height) < 60:
                continue

            # Do not offer the desktop/root background or shell surfaces as
            # application capture targets.
            if title.strip().lower() in {"desktop", "cinnamon", "panel"} or title.strip().lower().startswith("screenstory"):
                continue
            if shutil.which("xprop"):
                props = subprocess.run(
                    ["xprop", "-id", wid, "_NET_WM_WINDOW_TYPE"],
                    capture_output=True, text=True, check=False,
                ).stdout
                excluded_types = (
                    "_NET_WM_WINDOW_TYPE_DESKTOP",
                    "_NET_WM_WINDOW_TYPE_DOCK",
                    "_NET_WM_WINDOW_TYPE_NOTIFICATION",
                    "_NET_WM_WINDOW_TYPE_TOOLTIP",
                    "_NET_WM_WINDOW_TYPE_MENU",
                    "_NET_WM_WINDOW_TYPE_POPUP_MENU",
                )
                if any(kind in props for kind in excluded_types):
                    continue

            label = f"{title}  [{wid}]"
            self.window_choices[label] = (wid, int(width), int(height))
            labels.append(label)
        self.window_combo["values"] = labels
        if labels and self.selected_window.get() not in self.window_choices:
            self.selected_window.set(labels[0])
        self.window_hint.configure(text=f"{len(labels)} windows found")

    def build_capture_helper(self) -> Path:
        source = Path(__file__).with_name("xcomposite_capture.c")
        cache = Path.home() / ".cache" / "screenstory"
        cache.mkdir(parents=True, exist_ok=True)
        binary = cache / "xcomposite_capture"
        if binary.exists() and binary.stat().st_mtime >= source.stat().st_mtime:
            return binary
        if not shutil.which("cc") or not shutil.which("pkg-config"):
            raise RuntimeError("Window capture needs a C compiler and pkg-config. Install: sudo apt install build-essential pkg-config libx11-dev libxcomposite-dev")
        flags = subprocess.run(["pkg-config", "--cflags", "--libs", "x11", "xcomposite"],
                                capture_output=True, text=True, check=False)
        if flags.returncode != 0:
            raise RuntimeError("X11/XComposite development files are missing. Install: sudo apt install build-essential pkg-config libx11-dev libxcomposite-dev")
        command = ["cc", "-O2", "-Wall", "-Wextra", str(source), "-o", str(binary)]
        command.extend(flags.stdout.split())
        built = subprocess.run(command, capture_output=True, text=True, check=False)
        if built.returncode != 0:
            raise RuntimeError("Could not build XComposite helper:\n" + built.stderr)
        return binary

    def check_readiness(self) -> None:
        problems = []
        # Missing packages are handled at the point of use with an explicit
        # installation-consent dialog; do not disable the action or print sudo commands.
        if os.environ.get("XDG_SESSION_TYPE", "").lower() != "x11":
            problems.append("This prototype currently supports X11 sessions only.")
        if not os.environ.get("DISPLAY"):
            problems.append("DISPLAY is not set. Launch Screenstory from your desktop session.")
        try:
            screen_size()
        except Exception as exc:
            problems.append(str(exc))

        if problems:
            self.status.set("Not ready · " + " ".join(problems))
            self.record_button.configure(state="disabled")
        else:
            self.status.set("Ready · recordings are saved locally")
            self.record_button.configure(state="normal")

    def missing_packages(self, window_mode: bool) -> list[str]:
        """Return required Debian packages that are not installed."""
        packages = BASE_PACKAGES + (WINDOW_PACKAGES if window_mode else [])
        missing = []
        for package in packages:
            check = subprocess.run(
                ["dpkg-query", "-W", "-f", "${db:Status-Status}", package],
                capture_output=True, text=True, check=False,
            )
            if check.returncode != 0 or check.stdout.strip() != "installed":
                missing.append(package)
        return missing

    def install_missing_packages(self, packages: list[str]) -> None:
        """Ask once, then use PolicyKit for an explicit GUI-authorized install."""
        if not packages:
            return
        if not shutil.which("pkexec"):
            messagebox.showerror(
                "Cannot request installation",
                "PolicyKit (pkexec) is unavailable. Screenstory cannot request administrator approval on this system.",
            )
            return
        package_list = " ".join(packages)
        approved = messagebox.askyesno(
            "Screenstory needs additional components",
            "The following packages are missing:\n\n"
            + "\n".join(f"• {name}" for name in packages)
            + "\n\nScreenstory will ask Linux for administrator approval, "
              "then download and install these packages. Nothing will be installed unless you approve.\n\nContinue?",
        )
        if not approved:
            self.status.set("Installation cancelled · no system changes made")
            return

        self.record_button.configure(state="disabled")
        self.status.set("Waiting for administrator approval…")
        command = [
            "pkexec", "sh", "-c",
            "apt-get update && apt-get install -y " + package_list,
        ]

        def worker() -> None:
            try:
                result = subprocess.run(command, capture_output=True, text=True, check=False)
                detail = (result.stderr or result.stdout or "").strip()
                self.root.after(0, lambda: self.package_install_finished(result.returncode, detail))
            except Exception as exc:
                error_text = str(exc)
                self.root.after(0, lambda message=error_text: self.package_install_finished(1, message))

        threading.Thread(target=worker, daemon=True).start()

    def package_install_finished(self, return_code: int, detail: str) -> None:
        if return_code == 0:
            self.refresh_windows()
            self.status.set("Dependencies installed · starting recorder…")
            self.root.after(250, self.start_recording)
        else:
            self.status.set("Dependency installation was not completed")
            messagebox.showerror(
                "Installation not completed",
                "No recording was started. You can retry when ready.\n\n"
                + (detail[-2500:] if detail else "Administrator approval was denied or package installation failed."),
            )
            self.record_button.configure(state="normal")

    def choose_folder(self) -> None:
        selected = filedialog.askdirectory(initialdir=self.output_dir.get())
        if selected:
            self.output_dir.set(selected)

    def start_recording(self) -> None:
        if self.process is not None:
            return
        missing = self.missing_packages(self.source_mode.get() == "window")
        if missing:
            self.install_missing_packages(missing)
            return
        if os.environ.get("XDG_SESSION_TYPE", "").lower() != "x11":
            messagebox.showerror("Unsupported session", "This first prototype requires an X11 session.")
            return

        try:
            width, height = screen_size()
            window = None
            if self.source_mode.get() == "window":
                window = self.window_choices.get(self.selected_window.get())
                if not window:
                    raise RuntimeError("Select a window first, then click Refresh windows if needed.")
                width, height = window[1], window[2]
            output_dir = Path(self.output_dir.get()).expanduser()
            output_dir.mkdir(parents=True, exist_ok=True)
            stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
            self.output_file = output_dir / f"Screenstory-{stamp}.mp4"
            log_file = output_dir / f"Screenstory-{stamp}.log"
            self.log_handle = log_file.open("w", encoding="utf-8")

            if window:
                self.log_handle.write(
                    f"Screenstory FFmpeg X11 window capture\\nWindow ID: {window[0]}\\n"
                    f"Window title: {self.selected_window.get()}\\n"
                    f"Fixed output: {width}x{height}\\n"
                    "Resize behavior: source dimensions are fixed for this recording\\n"
                    f"Started: {dt.datetime.now().isoformat()}\\n"
                )
                self.log_handle.flush()
                command = [
                    "ffmpeg", "-hide_banner", "-y", "-loglevel", "warning",
                    "-f", "x11grab", "-framerate", "30", "-draw_mouse", "1",
                    "-video_size", f"{width}x{height}", "-window_id", window[0],
                    "-i", os.environ["DISPLAY"],
                    "-an", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                    "-c:v", "libx264", "-preset", "ultrafast",
                    "-pix_fmt", "yuv420p", "-movflags", "+faststart",
                    str(self.output_file),
                ]
                self.process = subprocess.Popen(
                    command, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                    stderr=self.log_handle, text=True,
                )
            else:
                command = [
                    "ffmpeg", "-hide_banner", "-y", "-loglevel", "warning",
                    "-f", "x11grab", "-framerate", "30", "-draw_mouse", "1",
                    "-video_size", f"{width}x{height}", "-i", os.environ["DISPLAY"],
                    "-an", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                    "-c:v", "libx264", "-preset", "ultrafast",
                    "-pix_fmt", "yuv420p", "-movflags", "+faststart",
                    str(self.output_file),
                ]
                self.process = subprocess.Popen(
                    command, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                    stderr=self.log_handle, text=True,
                )
            self.started_at = dt.datetime.now().timestamp()
            self.record_button.configure(state="disabled")
            self.stop_button.configure(state="normal")
            self.status.set("Recording · " + ("selected window" if window else "full desktop"))
            self.tick()
        except Exception as exc:
            self.cleanup_process()
            messagebox.showerror("Could not start recording", str(exc))
            self.check_readiness()

    def tick(self) -> None:
        if self.process is None or self.started_at is None:
            return
        seconds = max(0, int(dt.datetime.now().timestamp() - self.started_at))
        hours, remainder = divmod(seconds, 3600)
        minutes, secs = divmod(remainder, 60)
        self.elapsed.set(f"{hours:02d}:{minutes:02d}:{secs:02d}")
        if self.process.poll() is not None:
            self.finish_recording()
            return
        self.root.after(500, self.tick)

    def stop_recording(self) -> None:
        process = self.process
        if process is None:
            return
        self.stop_button.configure(state="disabled")
        self.status.set("Finishing MP4…")
        # Window capture uses FFmpeg stdin for raw frames, not commands.
        # Stop the producer first so EOF lets FFmpeg finalize the MP4.
        if self.capture_process is not None:
            if self.capture_process.poll() is None:
                self.capture_process.terminate()
        else:
            try:
                if process.stdin:
                    process.stdin.write("q\n")
                    process.stdin.flush()
            except (BrokenPipeError, OSError):
                pass
        threading.Thread(target=self.wait_for_encoder, daemon=True).start()

    def wait_for_encoder(self) -> None:
        process = self.process
        if process is not None:
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
        try:
            if self.root.winfo_exists():
                self.root.after(0, self.finish_recording)
        except tk.TclError:
            pass

    def cleanup_process(self) -> None:
        if self.capture_process is not None:
            if self.capture_process.poll() is None:
                self.capture_process.terminate()
            self.capture_process = None
        if self.log_handle:
            self.log_handle.close()
            self.log_handle = None
        self.process = None
        self.started_at = None

    def finish_recording(self) -> None:
        # Both the UI poller and stop-waiter may observe process exit.
        # Make finalization idempotent so the user gets one result dialog.
        process = self.process
        if process is None:
            return
        try:
            if not self.root.winfo_exists():
                return
        except tk.TclError:
            return
        output = self.output_file
        return_code = process.returncode
        self.cleanup_process()
        self.record_button.configure(state="normal")
        self.stop_button.configure(state="disabled")
        self.elapsed.set("00:00:00")

        if output and output.exists() and output.stat().st_size > 0 and return_code == 0:
            self.status.set(f"Saved · {output.name}")
            if self.closing:
                self.root.destroy()
            else:
                messagebox.showinfo(
                    "Recording saved",
                    f"Your recording is ready:\n\n{output}\n\nYou can open it with your video player.",
                )
        else:
            self.status.set("Recording stopped with an error. Check the log file.")
            if self.closing:
                self.root.destroy()
            else:
                messagebox.showerror(
                    "Recording error",
                    f"FFmpeg could not finish the recording.\nLog: {output.with_suffix('.log') if output else 'not available'}",
                )

    def on_close(self) -> None:
        if self.process is None:
            self.root.destroy()
            return
        if messagebox.askyesno("Stop recording?", "A recording is in progress. Stop and save it before closing?"):
            self.closing = True
            self.stop_recording()


def main() -> None:
    root = tk.Tk()
    ScreenstoryRecorder(root)
    root.mainloop()


if __name__ == "__main__":
    main()
