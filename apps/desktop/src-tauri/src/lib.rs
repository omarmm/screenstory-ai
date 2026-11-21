use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    runtime: &'static str,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Screenstory",
        version: env!("CARGO_PKG_VERSION"),
        runtime: "Tauri",
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_app_info])
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
}