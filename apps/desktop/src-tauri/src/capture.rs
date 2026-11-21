use serde::Serialize;
use x11rb::{
    connection::Connection,
    protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, MapState, Window},
    rust_connection::RustConnection,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X11Display {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X11Window {
    pub window_id: u32,
    pub title: String,
    pub app_hint: Option<String>,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub is_minimized: bool,
    pub is_visible: bool,
}

fn connect() -> Result<(RustConnection, usize, Window), String> {
    let (connection, screen_index) = x11rb::connect(None)
        .map_err(|_| "The X11 display connection is unavailable.".to_string())?;
    let root = connection
        .setup()
        .roots
        .get(screen_index)
        .ok_or_else(|| "The active X11 screen could not be resolved.".to_string())?
        .root;
    Ok((connection, screen_index, root))
}

pub fn display_dimensions() -> Result<X11Display, String> {
    let (connection, screen_index, _) = connect()?;
    let screen = connection
        .setup()
        .roots
        .get(screen_index)
        .ok_or_else(|| "The active X11 screen could not be resolved.".to_string())?;
    Ok(X11Display {
        width: u32::from(screen.width_in_pixels),
        height: u32::from(screen.height_in_pixels),
    })
}

fn intern(connection: &RustConnection, name: &[u8]) -> Result<Atom, String> {
    connection
        .intern_atom(false, name)
        .map_err(|_| "Could not query the X11 window metadata.".to_string())?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|_| "Could not query the X11 window metadata.".to_string())
}

fn property(
    connection: &RustConnection,
    window: Window,
    property_atom: Atom,
) -> Option<Vec<u8>> {
    connection
        .get_property(false, window, property_atom, AtomEnum::ANY, 0, 4096)
        .ok()?
        .reply()
        .ok()
        .map(|reply| reply.value)
}

fn property_u32(
    connection: &RustConnection,
    window: Window,
    property_atom: Atom,
) -> Option<u32> {
    let reply = connection
        .get_property(false, window, property_atom, AtomEnum::ANY, 0, 8)
        .ok()?
        .reply()
        .ok()?;
    let value = reply.value32()?.next();
    value
}

fn text_property(bytes: Option<Vec<u8>>) -> Option<String> {
    let bytes = bytes?;
    let value = String::from_utf8_lossy(&bytes);
    let cleaned = value
        .trim_matches('\0')
        .chars()
        .filter(|character| !character.is_control() || *character == '\t')
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() || cleaned.chars().count() > 512 {
        None
    } else {
        Some(cleaned)
    }
}

fn app_hint(
    connection: &RustConnection,
    window: Window,
    pid_atom: Atom,
    wm_class_atom: Atom,
) -> Option<String> {
    if let Some(pid) = property_u32(connection, window, pid_atom) {
        if pid > 1 {
            if let Ok(name) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
                let value = name.trim().to_string();
                if !value.is_empty() && value.len() <= 128 {
                    return Some(value);
                }
            }
        }
    }
    let value = property(connection, window, wm_class_atom)?;
    let class = String::from_utf8_lossy(&value)
        .split('\0')
        .rfind(|part| !part.trim().is_empty())?
        .trim()
        .to_string();
    if class.is_empty() || class.len() > 128 {
        None
    } else {
        Some(class)
    }
}

fn window_info(
    connection: &RustConnection,
    root: Window,
    window: Window,
    atoms: &WindowAtoms,
) -> Option<X11Window> {
    let attributes = connection.get_window_attributes(window).ok()?.reply().ok()?;
    let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
    if geometry.width < 80 || geometry.height < 50 {
        return None;
    }
    let position = connection
        .translate_coordinates(window, root, 0, 0)
        .ok()?
        .reply()
        .ok()?;

    let window_types = connection
        .get_property(false, window, atoms.window_type, AtomEnum::ANY, 0, 32)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .collect::<Vec<_>>();
    if window_types.contains(&atoms.dock) || window_types.contains(&atoms.desktop) {
        return None;
    }

    let states = connection
        .get_property(false, window, atoms.net_state, AtomEnum::ANY, 0, 32)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(|values| values.collect::<Vec<_>>()))
        .unwrap_or_default();
    let is_minimized = states.contains(&atoms.hidden);
    let is_visible = attributes.map_state == MapState::VIEWABLE && !is_minimized;

    let title = text_property(property(connection, window, atoms.net_name))
        .or_else(|| text_property(property(connection, window, atoms.wm_name)))
        .unwrap_or_else(|| format!("Window 0x{window:x}"));
    let app_hint = app_hint(connection, window, atoms.pid, atoms.wm_class);

    Some(X11Window {
        window_id: window,
        title,
        app_hint,
        width: u32::from(geometry.width),
        height: u32::from(geometry.height),
        x: i32::from(position.dst_x),
        y: i32::from(position.dst_y),
        is_minimized,
        is_visible,
    })
}

struct WindowAtoms {
    net_name: Atom,
    wm_name: Atom,
    net_state: Atom,
    hidden: Atom,
    window_type: Atom,
    dock: Atom,
    desktop: Atom,
    pid: Atom,
    wm_class: Atom,
}

fn load_atoms(connection: &RustConnection) -> Result<WindowAtoms, String> {
    Ok(WindowAtoms {
        net_name: intern(connection, b"_NET_WM_NAME")?,
        wm_name: AtomEnum::WM_NAME.into(),
        net_state: intern(connection, b"_NET_WM_STATE")?,
        hidden: intern(connection, b"_NET_WM_STATE_HIDDEN")?,
        window_type: intern(connection, b"_NET_WM_WINDOW_TYPE")?,
        dock: intern(connection, b"_NET_WM_WINDOW_TYPE_DOCK")?,
        desktop: intern(connection, b"_NET_WM_WINDOW_TYPE_DESKTOP")?,
        pid: intern(connection, b"_NET_WM_PID")?,
        wm_class: AtomEnum::WM_CLASS.into(),
    })
}

/// Enumerates EWMH top-level client windows. Window IDs are only descriptors;
/// callers must refresh and validate a selected ID immediately before capture.
pub fn list_x11_windows() -> Result<Vec<X11Window>, String> {
    let (connection, _, root) = connect()?;
    let atoms = load_atoms(&connection)?;
    let client_list = intern(&connection, b"_NET_CLIENT_LIST_STACKING")?;
    let mut windows = connection
        .get_property(false, root, client_list, AtomEnum::ANY, 0, 4096)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(|values| values.collect::<Vec<_>>()))
        .unwrap_or_default();

    if windows.is_empty() {
        windows = connection
            .query_tree(root)
            .map_err(|_| "Could not enumerate X11 desktop windows.".to_string())?
            .reply()
            .map_err(|_| "Could not enumerate X11 desktop windows.".to_string())?
            .children;
    }

    let mut result = windows
        .into_iter()
        .filter_map(|window| window_info(&connection, root, window, &atoms))
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.window_id.cmp(&right.window_id))
    });
    Ok(result)
}

pub fn get_window(window_id: u32) -> Result<Option<X11Window>, String> {
    if window_id == 0 {
        return Ok(None);
    }
    let (connection, _, root) = connect()?;
    let atoms = load_atoms(&connection)?;
    Ok(window_info(&connection, root, window_id, &atoms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_property_strips_control_chars_and_rejects_empty_values() {
        assert_eq!(
            text_property(Some(b"Firefox\0\0".to_vec())),
            Some("Firefox".to_string())
        );
        assert_eq!(
            text_property(Some(b"  Browser\nWindow  ".to_vec())),
            Some("BrowserWindow".to_string())
        );
        assert_eq!(text_property(Some(b"\0\0".to_vec())), None);
    }

    #[test]
    fn text_property_rejects_unbounded_window_titles() {
        let value = "x".repeat(513);
        assert_eq!(text_property(Some(value.into_bytes())), None);
    }

    #[test]
    fn display_dimension_fields_are_unsigned_and_serializable() {
        let display = X11Display { width: 1920, height: 1080 };
        let encoded = serde_json::to_string(&display).unwrap();
        assert!(encoded.contains("\"width\":1920"));
        assert!(encoded.contains("\"height\":1080"));
    }
}
