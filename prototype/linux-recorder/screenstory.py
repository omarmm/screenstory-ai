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
        self.log_handle = None
        self.output_file: Path | None = None
        self.started_at: float | None = None
        self.closing = False

        DEFAULT_OUTPUT.mkdir(parents=True, exist_ok=True)
        self.output_dir = tk.StringVar(value=str(DEFAULT_OUTPUT))
        self.status = tk.StringVar(value="Ready to record")
        self.elapsed = tk.StringVar(value="00:00:00")
        self.source_info = tk.StringVar(value=self.describe_source())

        self.build_ui()
        self.check_readiness()

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
        ttk.Label(
            source,
            text="First prototype: full desktop recording on X11.",
            foreground="#666666",
        ).pack(anchor="w", pady=(5, 0))

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

    def check_readiness(self) -> None:
        problems = []
        if not shutil.which("ffmpeg"):
            problems.append("FFmpeg was not found.")
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

    def choose_folder(self) -> None:
        selected = filedialog.askdirectory(initialdir=self.output_dir.get())
        if selected:
            self.output_dir.set(selected)

    def start_recording(self) -> None:
        if self.process is not None:
            return
        if not shutil.which("ffmpeg"):
            messagebox.showerror("FFmpeg missing", "Install FFmpeg, then reopen Screenstory.")
            return
        if os.environ.get("XDG_SESSION_TYPE", "").lower() != "x11":
            messagebox.showerror("Unsupported session", "This first prototype requires an X11 session.")
            return

        try:
            width, height = screen_size()
            output_dir = Path(self.output_dir.get()).expanduser()
            output_dir.mkdir(parents=True, exist_ok=True)
            stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
            self.output_file = output_dir / f"Screenstory-{stamp}.mp4"
            log_file = output_dir / f"Screenstory-{stamp}.log"
            self.log_handle = log_file.open("w", encoding="utf-8")

            command = [
                "ffmpeg", "-hide_banner", "-y", "-loglevel", "warning",
                "-f", "x11grab", "-framerate", "30", "-draw_mouse", "1",
                "-video_size", f"{width}x{height}", "-i", os.environ["DISPLAY"],
                "-an", "-c:v", "libx264", "-preset", "ultrafast",
                "-pix_fmt", "yuv420p", "-movflags", "+faststart",
                str(self.output_file),
            ]
            self.process = subprocess.Popen(
                command,
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=self.log_handle,
                text=True,
            )
            self.started_at = dt.datetime.now().timestamp()
            self.record_button.configure(state="disabled")
            self.stop_button.configure(state="normal")
            self.status.set("Recording · full desktop")
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
        self.root.after(0, self.finish_recording)

    def cleanup_process(self) -> None:
        if self.log_handle:
            self.log_handle.close()
            self.log_handle = None
        self.process = None
        self.started_at = None

    def finish_recording(self) -> None:
        process = self.process
        output = self.output_file
        return_code = process.returncode if process else None
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
