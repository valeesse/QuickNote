use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::open_popup;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub quick_note: String,
    pub clipboard_history: String,
    pub quick_note_alternate: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            quick_note: "Ctrl+Alt+N".into(),
            clipboard_history: "Ctrl+Alt+C".into(),
            quick_note_alternate: "Ctrl+Alt+Q".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ShortcutConfigInput {
    pub quick_note: String,
    pub clipboard_history: String,
    pub quick_note_alternate: String,
}

#[derive(Default)]
pub struct ShortcutRuntime {
    active: Mutex<ActiveShortcuts>,
}

#[derive(Clone, Default)]
struct ActiveShortcuts {
    quick_note: Option<Shortcut>,
    clipboard_history: Option<Shortcut>,
    quick_note_alternate: Option<Shortcut>,
}

impl ActiveShortcuts {
    fn values(&self) -> Vec<Shortcut> {
        [
            self.quick_note,
            self.clipboard_history,
            self.quick_note_alternate,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

pub struct ShortcutService {
    config_path: PathBuf,
    runtime: Arc<ShortcutRuntime>,
}

impl ShortcutService {
    pub fn new(config_path: PathBuf, runtime: Arc<ShortcutRuntime>) -> Self {
        Self {
            config_path,
            runtime,
        }
    }

    pub fn get_config(&self) -> ShortcutConfig {
        fs::read_to_string(&self.config_path)
            .ok()
            .and_then(|value| serde_json::from_str::<ShortcutConfig>(&value).ok())
            .unwrap_or_default()
    }

    pub fn set_config(
        &self,
        app: &AppHandle,
        input: ShortcutConfigInput,
    ) -> Result<ShortcutConfig, String> {
        let config = ShortcutConfig {
            quick_note: normalize_shortcut_label(&input.quick_note),
            clipboard_history: normalize_shortcut_label(&input.clipboard_history),
            quick_note_alternate: normalize_shortcut_label(&input.quick_note_alternate),
        };

        self.apply_config(app, &config, true)?;
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let json = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
        fs::write(&self.config_path, json).map_err(|error| error.to_string())?;
        Ok(config)
    }

    pub fn apply_config(
        &self,
        app: &AppHandle,
        config: &ShortcutConfig,
        strict: bool,
    ) -> Result<(), String> {
        let next = parse_config(config)?;
        let old = self.runtime.active.lock().unwrap().clone();
        unregister_all_known(app, old.values());

        let mut registered = ActiveShortcuts::default();
        let mut registered_values = Vec::new();

        for (slot, shortcut) in [
            ("quick_note", next.quick_note),
            ("clipboard_history", next.clipboard_history),
            ("quick_note_alternate", next.quick_note_alternate),
        ] {
            let Some(shortcut) = shortcut else {
                continue;
            };

            if let Err(error) = app.global_shortcut().register(shortcut) {
                if strict {
                    unregister_all_known(app, registered_values.clone());
                    restore_shortcuts(app, &old);
                    *self.runtime.active.lock().unwrap() = old;
                    return Err(format!(
                        "快捷键 {} 注册失败：{}",
                        shortcut.into_string(),
                        error
                    ));
                }
                eprintln!(
                    "failed to register shortcut {} for {slot}: {error}",
                    shortcut.into_string()
                );
                continue;
            }

            match slot {
                "quick_note" => registered.quick_note = Some(shortcut),
                "clipboard_history" => registered.clipboard_history = Some(shortcut),
                "quick_note_alternate" => registered.quick_note_alternate = Some(shortcut),
                _ => {}
            }
            registered_values.push(shortcut);
        }

        *self.runtime.active.lock().unwrap() = registered;
        Ok(())
    }
}

pub fn handle_shortcut(
    app: &AppHandle,
    runtime: &Arc<ShortcutRuntime>,
    shortcut: &Shortcut,
    event: ShortcutEvent,
) {
    if event.state != ShortcutState::Pressed {
        return;
    }

    let active = runtime.active.lock().unwrap().clone();
    if active.quick_note.as_ref() == Some(shortcut)
        || active.quick_note_alternate.as_ref() == Some(shortcut)
    {
        open_popup(app, "quick-note");
    } else if active.clipboard_history.as_ref() == Some(shortcut) {
        open_popup(app, "clipboard-popup");
    }
}

#[tauri::command]
pub fn get_shortcut_config(
    shortcuts: State<'_, Arc<ShortcutService>>,
) -> Result<ShortcutConfig, String> {
    Ok(shortcuts.get_config())
}

#[tauri::command]
pub fn set_shortcut_config(
    app: AppHandle,
    shortcuts: State<'_, Arc<ShortcutService>>,
    config: ShortcutConfigInput,
) -> Result<ShortcutConfig, String> {
    shortcuts.set_config(&app, config)
}

fn parse_config(config: &ShortcutConfig) -> Result<ActiveShortcuts, String> {
    let quick_note = parse_optional_shortcut("快速便签", &config.quick_note)?;
    let clipboard_history = parse_optional_shortcut("剪贴板历史", &config.clipboard_history)?;
    let quick_note_alternate =
        parse_optional_shortcut("备用快速便签", &config.quick_note_alternate)?;

    let mut seen = HashSet::new();
    for shortcut in [quick_note, clipboard_history, quick_note_alternate]
        .into_iter()
        .flatten()
    {
        if !seen.insert(shortcut.id()) {
            return Err(format!(
                "快捷键 {} 重复，请换一个组合",
                shortcut.into_string()
            ));
        }
    }

    Ok(ActiveShortcuts {
        quick_note,
        clipboard_history,
        quick_note_alternate,
    })
}

fn parse_optional_shortcut(label: &str, value: &str) -> Result<Option<Shortcut>, String> {
    let value = normalize_shortcut_label(value);
    if value.is_empty() {
        return Ok(None);
    }
    Shortcut::from_str(&value).map(Some).map_err(|error| {
        format!(
            "{}快捷键格式无效：{}。请使用 Ctrl+Alt+C 这类组合",
            label, error
        )
    })
}

fn normalize_shortcut_label(value: &str) -> String {
    value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("+")
}

fn unregister_all_known(app: &AppHandle, shortcuts: Vec<Shortcut>) {
    for shortcut in shortcuts {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

fn restore_shortcuts(app: &AppHandle, old: &ActiveShortcuts) {
    for shortcut in old.values() {
        let _ = app.global_shortcut().register(shortcut);
    }
}
