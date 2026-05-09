//! Temporary site-wide login gate.
//!
//! This is intentionally isolated so it can be removed later by deleting this
//! module and the routes/layer wiring in `routes.rs`.

use axum::{
    extract::Form,
    http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use once_cell::sync::Lazy;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

const TEMP_LOGIN_PASSWORD: &str = "142536";
const TEMP_LOGIN_COOKIE: &str = "scholarlens_temp_session";
const TEMP_LOGIN_SESSION_TTL_SECS: i64 = 12 * 60 * 60;

static TEMP_SESSIONS: Lazy<Mutex<HashMap<String, i64>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    password: String,
}

#[derive(Debug, Serialize)]
struct AuthError {
    error: &'static str,
}

pub async fn login_page() -> Html<&'static str> {
    Html(LOGIN_PAGE)
}

pub async fn login_handler(Form(form): Form<LoginForm>) -> Response {
    if !is_valid_password(&form.password) {
        return (StatusCode::UNAUTHORIZED, Html(LOGIN_FAILED_PAGE)).into_response();
    }

    let token = create_session();
    let cookie = format!(
        "{}={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax",
        TEMP_LOGIN_COOKIE, token, TEMP_LOGIN_SESSION_TTL_SECS
    );

    let mut response = Redirect::to("/").into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

pub async fn logout_handler(headers: HeaderMap) -> Response {
    if let Some(token) = session_token_from_headers(&headers) {
        if let Ok(mut sessions) = TEMP_SESSIONS.lock() {
            sessions.remove(&token);
        }
    }

    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "scholarlens_temp_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax",
        ),
    );
    response
}

pub async fn require_temp_login(req: Request<axum::body::Body>, next: Next) -> Response {
    if is_authenticated(req.headers()) {
        return next.run(req).await;
    }

    if req.method() == Method::GET && !is_api_path(req.uri().path()) && accepts_html(req.headers())
    {
        return Redirect::to("/login").into_response();
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(AuthError {
            error: "temporary login required",
        }),
    )
        .into_response()
}

fn is_valid_password(password: &str) -> bool {
    password
        .as_bytes()
        .ct_eq(TEMP_LOGIN_PASSWORD.as_bytes())
        .into()
}

fn create_session() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let expires_at = now_secs() + TEMP_LOGIN_SESSION_TTL_SECS;

    if let Ok(mut sessions) = TEMP_SESSIONS.lock() {
        cleanup_expired_sessions(&mut sessions);
        sessions.insert(token.clone(), expires_at);
    }

    token
}

fn is_authenticated(headers: &HeaderMap) -> bool {
    let Some(token) = session_token_from_headers(headers) else {
        return false;
    };

    let now = now_secs();
    let Ok(mut sessions) = TEMP_SESSIONS.lock() else {
        return false;
    };

    cleanup_expired_sessions(&mut sessions);
    match sessions.get_mut(&token) {
        Some(expires_at) if *expires_at >= now => {
            *expires_at = now + TEMP_LOGIN_SESSION_TTL_SECS;
            true
        }
        _ => false,
    }
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == TEMP_LOGIN_COOKIE).then(|| value.to_string())
    })
}

fn cleanup_expired_sessions(sessions: &mut HashMap<String, i64>) {
    let now = now_secs();
    sessions.retain(|_, expires_at| *expires_at >= now);
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn accepts_html(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|accept| accept.contains("text/html") || accept.contains("*/*"))
        .unwrap_or(false)
}

fn is_api_path(path: &str) -> bool {
    path == "/sources"
        || path == "/tasks"
        || path.starts_with("/tasks/")
        || path.starts_with("/api/")
}

const LOGIN_PAGE: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>ScholarLens 临时登录</title>
  <style>
    :root {
      color-scheme: light;
      --ink: #14213d;
      --muted: #5f6f89;
      --line: #d9e2ec;
      --card: rgba(255, 255, 255, 0.92);
      --accent: #0f766e;
      --accent-2: #f59e0b;
      --bg: #eef7f4;
    }
    * { box-sizing: border-box; }
    body {
      min-height: 100vh;
      margin: 0;
      display: grid;
      place-items: center;
      padding: 24px;
      color: var(--ink);
      font-family: "LXGW WenKai", "Noto Serif SC", Georgia, serif;
      background:
        radial-gradient(circle at 16% 18%, rgba(15, 118, 110, 0.24), transparent 28%),
        radial-gradient(circle at 82% 12%, rgba(245, 158, 11, 0.24), transparent 24%),
        linear-gradient(135deg, #f8fbf6 0%, var(--bg) 55%, #f7efe0 100%);
    }
    .card {
      width: min(100%, 420px);
      padding: 36px;
      border: 1px solid var(--line);
      border-radius: 28px;
      background: var(--card);
      box-shadow: 0 30px 90px rgba(20, 33, 61, 0.16);
      backdrop-filter: blur(14px);
    }
    .eyebrow {
      margin: 0 0 12px;
      color: var(--accent);
      font: 700 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace;
      letter-spacing: 0.16em;
      text-transform: uppercase;
    }
    h1 {
      margin: 0 0 10px;
      font-size: clamp(30px, 7vw, 48px);
      line-height: 1;
      letter-spacing: -0.05em;
    }
    p { margin: 0 0 26px; color: var(--muted); line-height: 1.7; }
    label {
      display: block;
      margin-bottom: 10px;
      font-weight: 700;
    }
    input {
      width: 100%;
      height: 52px;
      border: 1px solid var(--line);
      border-radius: 16px;
      padding: 0 16px;
      color: var(--ink);
      font: 700 22px/1 ui-monospace, SFMono-Regular, Menlo, monospace;
      letter-spacing: 0.18em;
      outline: none;
      background: #fff;
    }
    input:focus {
      border-color: var(--accent);
      box-shadow: 0 0 0 4px rgba(15, 118, 110, 0.14);
    }
    button {
      width: 100%;
      height: 52px;
      margin-top: 18px;
      border: 0;
      border-radius: 16px;
      color: white;
      cursor: pointer;
      font: 800 16px/1 ui-sans-serif, system-ui, sans-serif;
      background: linear-gradient(135deg, var(--accent), #115e59);
      box-shadow: 0 14px 28px rgba(15, 118, 110, 0.24);
    }
  </style>
</head>
<body>
  <main class="card">
    <p class="eyebrow">Temporary Gate</p>
    <h1>ScholarLens</h1>
    <p>当前站点已临时开启访问密码，登录后可继续使用搜索、历史记录和设置页面。</p>
    <form method="post" action="/login">
      <label for="password">访问密码</label>
      <input id="password" name="password" type="password" autocomplete="current-password" autofocus required>
      <button type="submit">进入 ScholarLens</button>
    </form>
  </main>
</body>
</html>"#;

const LOGIN_FAILED_PAGE: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>密码错误</title>
  <style>
    body {
      min-height: 100vh;
      margin: 0;
      display: grid;
      place-items: center;
      padding: 24px;
      color: #14213d;
      font-family: "LXGW WenKai", "Noto Serif SC", Georgia, serif;
      background: linear-gradient(135deg, #fff7ed, #eef7f4);
    }
    .card {
      width: min(100%, 420px);
      padding: 34px;
      border-radius: 26px;
      background: white;
      box-shadow: 0 24px 70px rgba(20, 33, 61, 0.14);
    }
    h1 { margin: 0 0 10px; font-size: 34px; }
    p { color: #5f6f89; line-height: 1.7; }
    a {
      display: inline-flex;
      margin-top: 16px;
      padding: 14px 18px;
      border-radius: 14px;
      color: white;
      text-decoration: none;
      font-weight: 800;
      background: #0f766e;
    }
  </style>
</head>
<body>
  <main class="card">
    <h1>密码错误</h1>
    <p>请输入正确的临时访问密码后再继续使用 ScholarLens。</p>
    <a href="/login">重新登录</a>
  </main>
</body>
</html>"#;
