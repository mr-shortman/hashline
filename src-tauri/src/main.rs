#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;

use cap_std::fs::Dir;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

struct Session {
    path: PathBuf,
    dir: Arc<Dir>,
    watcher: Option<RecommendedWatcher>,
}
#[derive(Default)]
struct Documents {
    sessions: Mutex<HashMap<String, Session>>,
    approved: Mutex<HashSet<PathBuf>>,
    pending: Mutex<Vec<String>>,
    sequence: AtomicU64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileDocument {
    id: String,
    path: String,
    name: String,
    source: String,
    read_ms: f64,
}
#[derive(Clone, Serialize)]
struct Change {
    id: String,
    failed: bool,
}
#[derive(Serialize)]
struct LinkResult {
    path: Option<String>,
    fragment: Option<String>,
}

fn authorize(state: &Documents, path: PathBuf) -> String {
    let mut approved = state.approved.lock().unwrap();
    if approved.len() >= 100 {
        approved.clear();
    }
    approved.insert(path.clone());
    path.to_string_lossy().into_owned()
}

fn enqueue(app: &tauri::AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<Documents>();
    let mut pending = state.pending.lock().unwrap();
    // Preserve invalid paths so the reader can report a useful error.
    pending.clear();
    for path in paths.into_iter().take(100) {
        let resolved = path.canonicalize().unwrap_or(path);
        pending.push(authorize(&state, resolved));
    }
    drop(pending);
    let _ = app.emit("open-request", ());
}

#[tauri::command]
fn take_open_requests(state: State<'_, Documents>) -> Vec<String> {
    std::mem::take(&mut *state.pending.lock().unwrap())
}

#[tauri::command]
async fn choose_file(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = app
            .dialog()
            .file()
            .add_filter(
                "Markdown",
                &["md", "markdown", "mdown", "mkd", "mkdn", "mdwn"],
            )
            .blocking_pick_files();
        let state = app.state::<Documents>();
        files
            .unwrap_or_default()
            .into_iter()
            .map(|file| {
                let path = file
                    .into_path()
                    .map_err(|_| "Ungültiger Dateipfad.".to_string())?;
                let path = files::markdown_path(&path)?;
                Ok(authorize(&state, path))
            })
            .collect()
    })
    .await
    .map_err(|_| "Der Dateidialog wurde unterbrochen.".to_string())?
}

#[tauri::command]
async fn read_document(app: tauri::AppHandle, path: String) -> Result<FileDocument, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let start = Instant::now();
        let path = PathBuf::from(path);
        let state = app.state::<Documents>();
        let existing = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .values()
                .find(|s| s.path == path)
                .map(|s| s.dir.clone())
        };
        if !state.approved.lock().unwrap().contains(&path) && existing.is_none() {
            return Err("Diese Datei wurde nicht zum Öffnen freigegeben.".into());
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !["md", "markdown", "mdown", "mkd", "mkdn", "mdwn"].contains(&extension.as_str()) {
            return Err("Das Ziel ist keine unterstützte Markdown-Datei.".into());
        }
        let parent = path.parent().ok_or("Ungültiger Dateipfad.")?;
        let dir = match existing {
            Some(dir) => dir,
            None => Arc::new(
                Dir::open_ambient_dir(parent, cap_std::ambient_authority())
                    .map_err(|_| "Das Verzeichnis ist nicht lesbar.".to_string())?,
            ),
        };
        let name = path.file_name().ok_or("Ungültiger Dateiname.")?;
        let bytes = files::read_bounded(&dir, Path::new(name), files::MARKDOWN_LIMIT)?;
        let source = String::from_utf8(bytes)
            .map_err(|_| "Die Datei ist nicht gültig UTF-8-kodiert.".to_string())?;
        let source = source
            .strip_prefix('\u{feff}')
            .unwrap_or(&source)
            .to_owned();
        let id = state.sequence.fetch_add(1, Ordering::Relaxed).to_string();
        let result = FileDocument {
            id: id.clone(),
            path: path.to_string_lossy().into_owned(),
            name: name.to_string_lossy().into_owned(),
            source,
            read_ms: start.elapsed().as_secs_f64() * 1000.0,
        };
        let mut sessions = state.sessions.lock().unwrap();
        if sessions.len() >= 8 {
            return Err("Zu viele gleichzeitige Dateiöffnungen. Bitte erneut versuchen.".into());
        }
        sessions.insert(
            id,
            Session {
                path,
                dir,
                watcher: None,
            },
        );
        Ok(result)
    })
    .await
    .map_err(|_| "Das Lesen wurde unterbrochen.".to_string())?
}

#[tauri::command]
fn release_document(state: State<'_, Documents>, id: String) {
    state.sessions.lock().unwrap().remove(&id);
}
#[tauri::command]
fn unwatch_document(state: State<'_, Documents>, id: String) {
    if let Some(session) = state.sessions.lock().unwrap().get_mut(&id) {
        session.watcher = None;
    }
}

#[tauri::command]
fn watch_document(
    app: tauri::AppHandle,
    state: State<'_, Documents>,
    id: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().unwrap();
    let session = sessions.get_mut(&id).ok_or("Dokument wurde geschlossen.")?;
    let target = session.path.clone();
    let mut watcher = notify::recommended_watcher(
        move |event: Result<notify::Event, notify::Error>| match event {
            Ok(event)
                if !matches!(event.kind, notify::EventKind::Access(_))
                    && event.paths.contains(&target) =>
            {
                let _ = app.emit(
                    "document-changed",
                    Change {
                        id: id.clone(),
                        failed: false,
                    },
                );
            }
            Err(_) => {
                let _ = app.emit(
                    "document-changed",
                    Change {
                        id: id.clone(),
                        failed: true,
                    },
                );
            }
            _ => {}
        },
    )
    .map_err(|_| "Dateibeobachtung nicht verfügbar.".to_string())?;
    watcher
        .watch(
            session.path.parent().ok_or("Ungültiges Verzeichnis.")?,
            RecursiveMode::NonRecursive,
        )
        .map_err(|_| "Verzeichnis kann nicht beobachtet werden.".to_string())?;
    session.watcher = Some(watcher);
    Ok(())
}

#[tauri::command]
async fn follow_link(
    app: tauri::AppHandle,
    id: String,
    href: String,
) -> Result<LinkResult, String> {
    if href.len() > 8192 || href.chars().any(char::is_control) {
        return Err("Ungültiger Link.".into());
    }
    if let Ok(url) = url::Url::parse(&href) {
        if !["https", "http", "mailto"].contains(&url.scheme()) {
            return Err("Dieses Link-Schema wird nicht unterstützt.".into());
        }
        app.opener()
            .open_url(url.as_str(), None::<&str>)
            .map_err(|_| "Die Systemanwendung konnte nicht geöffnet werden.".to_string())?;
        return Ok(LinkResult {
            path: None,
            fragment: None,
        });
    }
    if href.starts_with("//") {
        return Err("Netzwerkpfade sind nicht freigegeben.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Documents>();
        let base = state
            .sessions
            .lock()
            .unwrap()
            .get(&id)
            .ok_or("Dokument wurde geschlossen.")?
            .path
            .clone();
        let path = files::decode_relative_url(&href)?;
        // A user click is an explicit open operation, unlike automatic image resolution.
        let path =
            files::markdown_path(&base.parent().ok_or("Ungültiges Verzeichnis.")?.join(path))?;
        let fragment = href
            .split_once('#')
            .map(|(_, fragment)| fragment.to_owned());
        Ok(LinkResult {
            path: Some(authorize(&state, path)),
            fragment,
        })
    })
    .await
    .map_err(|_| "Der Link konnte nicht geöffnet werden.".to_string())?
}

#[tauri::command]
fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    if text.len() > files::MARKDOWN_LIMIT as usize {
        return Err("Der Text ist zu groß für die Zwischenablage.".into());
    }
    app.clipboard()
        .write_text(text)
        .map_err(|_| "Die Zwischenablage ist nicht verfügbar.".into())
}

fn main() {
    let started = Instant::now();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            enqueue(
                app,
                args.into_iter()
                    .skip(1)
                    .map(|arg| Path::new(&cwd).join(arg))
                    .collect(),
            );
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("navigation-policy")
                .on_navigation(|_, url| {
                    (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                        || (cfg!(debug_assertions)
                            && url.scheme() == "http"
                            && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
                            && url.port() == Some(1420))
                })
                .build(),
        )
        .manage(Documents::default())
        .register_asynchronous_uri_scheme_protocol(
            "hashline-image",
            |context, request, responder| {
                let app = context.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let result = (|| {
                        let path = request.uri().path().trim_start_matches('/');
                        let (id, source) = path.split_once('/').ok_or("Ungültige Ressource.")?;
                        let source = percent_encoding::percent_decode_str(source)
                            .decode_utf8()
                            .map_err(|_| "Ungültige Ressource.")?;
                        let state = app.state::<Documents>();
                        let dir = state
                            .sessions
                            .lock()
                            .unwrap()
                            .get(id)
                            .ok_or("Dokument geschlossen.")?
                            .dir
                            .clone();
                        files::image_bytes(&dir, &source)
                    })();
                    let response = match result {
                        Ok((body, mime)) => tauri::http::Response::builder()
                            .header("Content-Type", mime)
                            .header("Cache-Control", "no-store")
                            .header("X-Content-Type-Options", "nosniff")
                            .body(body),
                        Err(_) => tauri::http::Response::builder()
                            .status(403)
                            .header("Content-Type", "text/plain")
                            .body(Vec::new()),
                    };
                    if let Ok(response) = response {
                        responder.respond(response);
                    }
                });
            },
        )
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                enqueue(window.app_handle(), paths.clone());
            }
        })
        .setup(move |app| {
            let cwd = std::env::current_dir()?;
            enqueue(
                app.handle(),
                std::env::args_os()
                    .skip(1)
                    .filter(|arg| arg != "--")
                    .map(|arg| cwd.join(arg))
                    .collect(),
            );
            // Diagnostic timing contains no private path or document contents.
            if std::env::var_os("HASHLINE_DIAGNOSTICS").is_some() {
                eprintln!(
                    "hashline.native-setup-ms={:.2}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            choose_file,
            read_document,
            release_document,
            watch_document,
            unwatch_document,
            follow_link,
            copy_text,
            take_open_requests
        ])
        .run(tauri::generate_context!())
        .expect("Hashline konnte nicht gestartet werden");
}
