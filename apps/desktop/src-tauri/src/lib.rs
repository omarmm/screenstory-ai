use serde::Serialize;
use std::process::Command;

const FFMPEG_PACKAGE: &str = "ffmpeg";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    runtime: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyStatus {
    id: &'static str,
    feature: &'static str,
    status: &'static str,
    version: Option<String>,
    impact: &'static str,
    can_install: bool,
    remediation: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyReport {
    dependencies: Vec<DependencyStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallResult {
    success: bool,
    message: String,
    report: DependencyReport,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Screenstory",
        version: env!("CARGO_PKG_VERSION"),
        runtime: "Tauri",
    }
}

fn find_executable(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(name).is_file()))
        .unwrap_or(false)
}

fn ffmpeg_status() -> DependencyStatus {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => {
            let first_line = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("FFmpeg available")
                .to_string();
            DependencyStatus {
                id: "ffmpeg",
                feature: "Recording and video processing",
                status: "available",
                version: Some(first_line),
                impact: "Recording, frame extraction, and video export are available.",
                can_install: false,
                remediation: None,
            }
        }
        Ok(_) | Err(_) => DependencyStatus {
            id: "ffmpeg",
            feature: "Recording and video processing",
            status: "missing",
            version: None,
            impact:
                "Screen recording and video processing cannot be used until FFmpeg is installed.",
            can_install: cfg!(target_os = "linux")
                && find_executable("pkexec")
                && find_executable("apt-get"),
            remediation: Some(
                "Install the FFmpeg package from your Linux distribution repositories.",
            ),
        },
    }
}

fn dependency_report() -> DependencyReport {
    DependencyReport {
        dependencies: vec![ffmpeg_status()],
    }
}

#[tauri::command]
fn check_dependencies() -> DependencyReport {
    dependency_report()
}

#[tauri::command]
fn install_ffmpeg() -> InstallResult {
    #[cfg(target_os = "linux")]
    {
        if !find_executable("pkexec") || !find_executable("apt-get") {
            return InstallResult {
                success: false,
                message:
                    "The system package authorization service is unavailable. No changes were made."
                        .into(),
                report: dependency_report(),
            };
        }

        // Fixed executable and allowlisted package only. No shell and no frontend-supplied arguments.
        let result = Command::new("pkexec")
            .arg("apt-get")
            .arg("install")
            .arg("-y")
            .arg(FFMPEG_PACKAGE)
            .output();

        let message = match result {
            Ok(output) if output.status.success() => {
                "Package installation finished. Verifying FFmpeg…".to_string()
            }
            Ok(output) => {
                let details = String::from_utf8_lossy(&output.stderr);
                if details.contains("Could not get lock") || details.contains("Unable to acquire") {
                    "The system package manager is busy. Close other package managers and try again.".into()
                } else if output.status.code() == Some(126) || output.status.code() == Some(127) {
                    "Installation was cancelled or authorization was denied. No feature was enabled.".into()
                } else {
                    "FFmpeg installation did not complete. Check your network and software sources, then try again.".into()
                }
            }
            Err(_) => "Could not start the system package installer. No changes were made.".into(),
        };
        let report = dependency_report();
        let success = report
            .dependencies
            .first()
            .is_some_and(|item| item.status == "available");
        return InstallResult {
            success,
            message: if success {
                "FFmpeg is installed and verified.".into()
            } else {
                message
            },
            report,
        };
    }

    #[cfg(not(target_os = "linux"))]
    {
        InstallResult {
            success: false,
            message: "Automatic FFmpeg installation is currently supported only on Linux Mint."
                .into(),
            report: dependency_report(),
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            check_dependencies,
            install_ffmpeg
        ])
        .run(tauri::generate_context!())
        .expect("error while running Screenstory");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_exposes_expected_identity() {
        let info = get_app_info();
        assert_eq!(info.name, "Screenstory");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.runtime, "Tauri");
    }

    #[test]
    fn dependency_report_has_ffmpeg_feature_contract() {
        let report = dependency_report();
        let ffmpeg = report.dependencies.first().expect("FFmpeg entry");
        assert_eq!(ffmpeg.id, "ffmpeg");
        assert_eq!(ffmpeg.feature, "Recording and video processing");
        assert!(["available", "missing"].contains(&ffmpeg.status));
    }

    #[test]
    fn executable_lookup_does_not_accept_empty_path() {
        assert!(!find_executable("screenstory-definitely-not-an-executable"));
    }
}
