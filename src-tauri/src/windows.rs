use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mcdh_core::{AppSettings, ComponentSummary};
use serde::{Deserialize, Serialize};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum DialogRequest {
    Settings,
    Create,
    Import,
    Warnings,
    Component {
        #[serde(rename = "componentId")]
        component_id: String,
        #[serde(
            rename = "initialComponent",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        initial_component: Option<Box<ComponentSummary>>,
        #[serde(
            rename = "initialSettings",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        initial_settings: Option<AppSettings>,
    },
    Startup {
        dialog: serde_json::Value,
    },
    Confirm {
        message: String,
        token: String,
        owner: String,
    },
}

impl DialogRequest {
    fn specification(&self) -> (String, &'static str, f64, f64) {
        match self {
            Self::Settings => ("dialog-settings".into(), "设置", 920., 720.),
            Self::Create => ("dialog-create".into(), "新建组件", 600., 640.),
            Self::Import => ("dialog-import".into(), "导入组件", 620., 660.),
            Self::Warnings => ("dialog-warnings".into(), "扫描问题", 760., 620.),
            Self::Component { component_id, .. } => {
                let mut hash = DefaultHasher::new();
                component_id.hash(&mut hash);
                (
                    format!("dialog-component-{:x}", hash.finish()),
                    "组件配置",
                    800.,
                    740.,
                )
            }
            Self::Startup { .. } => ("dialog-startup".into(), "版本更新", 620., 550.),
            Self::Confirm { token, .. } => {
                let mut hash = DefaultHasher::new();
                token.hash(&mut hash);
                (
                    format!("dialog-confirm-{:x}", hash.finish()),
                    "确认操作",
                    500.,
                    300.,
                )
            }
        }
    }
}

// Window creation must run asynchronously on Windows to avoid WebView2 IPC deadlocks.
#[tauri::command]
pub(crate) async fn open_dialog_window(
    app: tauri::AppHandle,
    request: DialogRequest,
) -> Result<String, String> {
    let (label, title, width, height) = request.specification();
    if let Some(existing) = app.get_webview_window(&label) {
        existing.unminimize().map_err(|error| error.to_string())?;
        existing.set_focus().map_err(|error| error.to_string())?;
        return Ok(label);
    }
    let parent_label = match &request {
        DialogRequest::Confirm { owner, .. } => owner.as_str(),
        _ => "main",
    };
    let parent = app.get_webview_window(parent_label).ok_or("父窗口已关闭")?;
    let payload = serde_json::to_string(&request).map_err(|error| error.to_string())?;
    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("{title} · MCDH"))
        .inner_size(width, height)
        .min_inner_size(420., 280.)
        .decorations(false)
        .shadow(true)
        .center()
        .visible(false)
        .initialization_script(format!("window.__MCDH_DIALOG__ = {payload};"))
        .parent(&parent)
        .map_err(|error| error.to_string())?;
    if let Some(arguments) = app
        .config()
        .app
        .windows
        .first()
        .and_then(|window| window.additional_browser_args.as_ref())
    {
        builder = builder.additional_browser_args(arguments);
    }
    let child = builder.build().map_err(|error| error.to_string())?;
    if matches!(request, DialogRequest::Confirm { .. }) {
        parent
            .set_enabled(false)
            .map_err(|error| error.to_string())?;
        let owner = parent.clone();
        child.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = owner.set_enabled(true);
                let _ = owner.set_focus();
            }
        });
    }
    if let (Ok(position), Ok(size), Ok(child_size)) = (
        parent.outer_position(),
        parent.outer_size(),
        child.outer_size(),
    ) {
        let mut x = position.x + (size.width as i32 - child_size.width as i32) / 2;
        let mut y = position.y + (size.height as i32 - child_size.height as i32) / 2;
        if let Ok(Some(monitor)) = parent.current_monitor() {
            let area = monitor.work_area();
            let width = child_size.width.min(area.size.width);
            let height = child_size.height.min(area.size.height);
            let _ = child.set_size(tauri::PhysicalSize::new(width, height));
            x = x.clamp(
                area.position.x,
                area.position.x + (area.size.width - width) as i32,
            );
            y = y.clamp(
                area.position.y,
                area.position.y + (area.size.height - height) as i32,
            );
        }
        let _ = child.set_position(tauri::PhysicalPosition::new(x, y));
    }
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_have_stable_labels_and_component_windows_are_distinct() {
        let one = DialogRequest::Component {
            component_id: "D:/中文/project".into(),
            initial_component: None,
            initial_settings: None,
        };
        let two = DialogRequest::Component {
            component_id: "another".into(),
            initial_component: None,
            initial_settings: None,
        };
        assert_eq!(one.specification().0, one.specification().0);
        assert_ne!(one.specification().0, two.specification().0);
        assert!(
            one.specification()
                .0
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        );
        assert_eq!(DialogRequest::Settings.specification().0, "dialog-settings");
    }

    #[test]
    fn unknown_window_kinds_are_rejected() {
        assert!(
            serde_json::from_str::<DialogRequest>(r#"{"kind":"https://example.com"}"#).is_err()
        );
        let request: DialogRequest =
            serde_json::from_str(r#"{"kind":"component","componentId":"abc"}"#).unwrap();
        assert!(
            matches!(request, DialogRequest::Component { component_id, .. } if component_id == "abc")
        );
    }
}
