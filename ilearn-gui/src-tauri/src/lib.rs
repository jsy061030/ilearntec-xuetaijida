use std::path::PathBuf;

use base64::Engine as _;
use serde::Serialize;
use tauri::{Manager, State};
use tokio::sync::Mutex;

use ilearn_core::api::ApiClient;
use ilearn_core::auth::{AuthClient, PasswordOutcome, QrPoll, Session, WxCodeStatus};
use ilearn_core::models::{Lesson, Record, Term, User, VideoResource};

mod proxy;

struct AppState {
    auth: Mutex<AuthClient>,
    inner: Mutex<Inner>,
    session_path: PathBuf,
    proxy_base: String,
}

struct Inner {
    session: Option<Session>,
    qr_uuid: Option<String>,
    pending_lt: Option<String>,
    pending_execution: Option<String>,
}

#[derive(Serialize)]
struct QrStart {
    image: String,
}

#[derive(Serialize)]
struct QrStatus {
    status: String,
    user: Option<User>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordStart {
    need_second_factor: bool,
    user: Option<User>,
    captcha: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    current_version: String,
    latest_version: String,
    has_update: bool,
    url: String,
    name: Option<String>,
    published_at: Option<String>,
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let latest = latest.trim_start_matches('v');
    let current = current.trim_start_matches('v');
    match (
        semver::Version::parse(latest),
        semver::Version::parse(current),
    ) {
        (Ok(latest), Ok(current)) => latest > current,
        _ => latest != current,
    }
}

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn check_update() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("ilearn-gui-updater")
        .build()
        .map_err(err)?;
    let resp = client
        .get("https://api.github.com/repos/jsy061030/ilearntec-xuetaijida/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(err)?;
    let status = resp.status();
    let text = resp.text().await.map_err(err)?;
    if !status.is_success() {
        return Err(format!(
            "GitHub API HTTP {status}: {}",
            ilearn_core::util::safe_truncate(&text, 200)
        ));
    }
    let json: serde_json::Value = serde_json::from_str(&text).map_err(err)?;
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "GitHub Release 缺少 tag_name".to_string())?;
    let latest_version = tag.trim_start_matches('v').to_string();
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    Ok(UpdateInfo {
        has_update: version_is_newer(&latest_version, &current_version),
        current_version,
        latest_version,
        url: json
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://github.com/jsy061030/ilearntec-xuetaijida/releases")
            .to_string(),
        name: json
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        published_at: json
            .get("published_at")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

async fn save_session(auth: &AuthClient, path: &PathBuf) {
    if let Err(e) = auth.save_session(path) {
        log::warn!("保存会话失败: {e:#}");
    }
}

#[tauri::command]
async fn load_session(state: State<'_, AppState>) -> Result<Option<User>, String> {
    let auth = state.auth.lock().await;
    let mut inner = state.inner.lock().await;

    match auth.load_session(&state.session_path) {
        Ok(true) => match auth.cached_session().await {
            Some(session) => {
                let user = session.user.clone();
                inner.session = Some(session);
                Ok(Some(user))
            }
            None => {
                drop(inner);
                drop(auth);
                let mut auth = state.auth.lock().await;
                let _ = auth.reset();
                Ok(None)
            }
        },
        _ => Ok(None),
    }
}

#[tauri::command]
async fn start_qr(state: State<'_, AppState>) -> Result<QrStart, String> {
    let mut auth = state.auth.lock().await;
    auth.reset().map_err(err)?;
    let _ = std::fs::remove_file(&state.session_path);
    let (uuid, path) = auth.generate_qr().await.map_err(err)?;
    let mut inner = state.inner.lock().await;
    inner.session = None;
    inner.qr_uuid = Some(uuid);
    inner.pending_lt = None;
    inner.pending_execution = None;

    let bytes = std::fs::read(&path).map_err(err)?;
    Ok(QrStart {
        image: data_url("image/png", &bytes),
    })
}

#[tauri::command]
async fn poll_qr(state: State<'_, AppState>) -> Result<QrStatus, String> {
    let auth = state.auth.lock().await;
    let uuid = {
        let inner = state.inner.lock().await;
        inner.qr_uuid.clone().ok_or_else(|| "请先生成二维码".to_string())?
    };

    match auth.poll_qr(&uuid).await.map_err(err)? {
        QrPoll::Pending => Ok(QrStatus {
            status: "pending".into(),
            user: None,
        }),
        QrPoll::Expired => Ok(QrStatus {
            status: "expired".into(),
            user: None,
        }),
        QrPoll::Scanned(session) => {
            let user = session.user.clone();
            let mut inner = state.inner.lock().await;
            inner.session = Some(session);
            inner.qr_uuid = None;
            save_session(&auth, &state.session_path).await;
            Ok(QrStatus {
                status: "scanned".into(),
                user: Some(user),
            })
        }
    }
}

#[tauri::command]
async fn start_password(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<PasswordStart, String> {
    let mut auth = state.auth.lock().await;
    auth.reset().map_err(err)?;
    let _ = std::fs::remove_file(&state.session_path);

    let form = auth.fetch_login_form().await.map_err(err)?;
    match auth
        .submit_password(&form, &username, &password)
        .await
        .map_err(err)?
    {
        PasswordOutcome::Complete(session) => {
            let user = session.user.clone();
            {
                let mut inner = state.inner.lock().await;
                inner.session = Some(session);
                inner.pending_lt = None;
                inner.pending_execution = None;
            }
            save_session(&auth, &state.session_path).await;
            Ok(PasswordStart {
                need_second_factor: false,
                user: Some(user),
                captcha: None,
            })
        }
        PasswordOutcome::NeedSecondFactor { lt, execution } => {
            let captcha_path = auth.fetch_captcha().await.map_err(err)?;
            let bytes = std::fs::read(&captcha_path).map_err(err)?;
            let mime = if captcha_path.extension().and_then(|s| s.to_str()) == Some("png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            {
                let mut inner = state.inner.lock().await;
                inner.pending_lt = Some(lt);
                inner.pending_execution = Some(execution);
            }
            Ok(PasswordStart {
                need_second_factor: true,
                user: None,
                captcha: Some(data_url(mime, &bytes)),
            })
        }
    }
}

#[tauri::command]
async fn refresh_captcha(state: State<'_, AppState>) -> Result<String, String> {
    let auth = state.auth.lock().await;
    let path = auth.fetch_captcha().await.map_err(err)?;
    let bytes = std::fs::read(&path).map_err(err)?;
    let mime = if path.extension().and_then(|s| s.to_str()) == Some("png") {
        "image/png"
    } else {
        "image/jpeg"
    };
    Ok(data_url(mime, &bytes))
}

#[tauri::command]
async fn request_wx(state: State<'_, AppState>, captcha: String) -> Result<String, String> {
    let auth = state.auth.lock().await;
    match auth.request_wx_code(&captcha).await.map_err(err)? {
        WxCodeStatus::Sent => Ok("sent".into()),
        WxCodeStatus::Unknown => Ok("unknown".into()),
    }
}

#[tauri::command]
async fn submit_wx(
    state: State<'_, AppState>,
    captcha: String,
    wx_code: String,
) -> Result<User, String> {
    let auth = state.auth.lock().await;
    let (lt, execution) = {
        let inner = state.inner.lock().await;
        (
            inner
                .pending_lt
                .clone()
                .ok_or_else(|| "缺少登录状态，请重新提交密码".to_string())?,
            inner
                .pending_execution
                .clone()
                .ok_or_else(|| "缺少登录状态，请重新提交密码".to_string())?,
        )
    };

    let session = auth
        .submit_verification(&lt, &execution, &captcha, &wx_code)
        .await
        .map_err(err)?;
    let user = session.user.clone();
    {
        let mut inner = state.inner.lock().await;
        inner.session = Some(session);
        inner.pending_lt = None;
        inner.pending_execution = None;
    }
    save_session(&auth, &state.session_path).await;
    Ok(user)
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    let mut auth = state.auth.lock().await;
    auth.reset().map_err(err)?;
    let mut inner = state.inner.lock().await;
    inner.session = None;
    inner.qr_uuid = None;
    inner.pending_lt = None;
    inner.pending_execution = None;
    let _ = std::fs::remove_file(&state.session_path);
    Ok(())
}

async fn session_client(state: &State<'_, AppState>) -> Result<reqwest::Client, String> {
    let inner = state.inner.lock().await;
    inner
        .session
        .as_ref()
        .map(|s| s.client.clone())
        .ok_or_else(|| "未登录".to_string())
}

#[tauri::command]
fn get_proxy_base(state: State<'_, AppState>) -> String {
    state.proxy_base.clone()
}

#[tauri::command]
async fn get_terms(state: State<'_, AppState>) -> Result<Vec<Term>, String> {
    let client = session_client(&state).await?;
    ApiClient::new(client).terms().await.map_err(err)
}

#[tauri::command]
async fn get_lessons(
    state: State<'_, AppState>,
    term_year: String,
    term: String,
) -> Result<Vec<Lesson>, String> {
    let client = session_client(&state).await?;
    ApiClient::new(client)
        .lessons(&term_year, &term)
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_records(
    state: State<'_, AppState>,
    term_id: String,
    lesson_id: String,
) -> Result<Vec<Record>, String> {
    let client = session_client(&state).await?;
    ApiClient::new(client)
        .records(&term_id, &lesson_id)
        .await
        .map_err(err)
}

#[tauri::command]
async fn get_video(
    state: State<'_, AppState>,
    resource_id: String,
) -> Result<VideoResource, String> {
    let client = session_client(&state).await?;
    let api = ApiClient::new(client);
    api.ensure_res_session().await.map_err(err)?;
    api.video_resource(&resource_id).await.map_err(err)
}

#[tauri::command]
async fn get_subtitle(state: State<'_, AppState>, url: String) -> Result<String, String> {
    let client = session_client(&state).await?;
    let bytes = ApiClient::new(client)
        .download_bytes(&url)
        .await
        .map_err(err)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let cache = app.path().app_cache_dir()?.join("ilearn");
            std::fs::create_dir_all(&cache)?;
            let auth = AuthClient::new(&cache)?;
            let proxy_base = proxy::spawn(auth.jar())?;
            log::info!("媒体代理: {proxy_base}");
            app.manage(AppState {
                auth: Mutex::new(auth),
                inner: Mutex::new(Inner {
                    session: None,
                    qr_uuid: None,
                    pending_lt: None,
                    pending_execution: None,
                }),
                session_path: cache.join("session.json"),
                proxy_base,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_session,
            start_qr,
            poll_qr,
            start_password,
            refresh_captcha,
            request_wx,
            submit_wx,
            logout,
            get_proxy_base,
            app_version,
            check_update,
            get_terms,
            get_lessons,
            get_records,
            get_video,
            get_subtitle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
