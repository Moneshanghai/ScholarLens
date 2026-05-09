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
  <title>ScholarLens · 登录</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap-icons@1.11.3/font/bootstrap-icons.min.css">
  <style>
    :root {
      color-scheme: light;
      --primary: #4361ee;
      --primary-hover: #3a56d4;
      --primary-dark: #3a0ca3;
      --gradient-brand: linear-gradient(135deg, #4361ee 0%, #3a0ca3 100%);
      --gradient-brand-hover: linear-gradient(135deg, #3a56d4 0%, #2f088b 100%);
      --bg: #f4f6fb;
      --surface: #ffffff;
      --border: #e4e7ee;
      --border-strong: #cbd5e1;
      --text: #1f2937;
      --text-strong: #0f172a;
      --text-muted: #64748b;
      --text-soft: #94a3b8;
      --danger: #ef4444;
      --danger-soft: #fef2f2;
      --shadow-brand: 0 6px 16px rgba(67, 97, 238, 0.22);
      --shadow-card: 0 12px 32px rgba(15, 23, 42, 0.10);
      --font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
        "Helvetica Neue", Arial, "PingFang SC", "Hiragino Sans GB",
        "Microsoft YaHei", "Noto Sans CJK SC", sans-serif;
    }
    * { box-sizing: border-box; }
    html, body { height: 100%; }
    body {
      margin: 0;
      min-height: 100vh;
      color: var(--text);
      font-family: var(--font-family);
      font-size: 0.9375rem;
      line-height: 1.6;
      background:
        radial-gradient(circle at 12% 14%, rgba(67, 97, 238, 0.18), transparent 38%),
        radial-gradient(circle at 88% 18%, rgba(58, 12, 163, 0.20), transparent 40%),
        radial-gradient(circle at 78% 92%, rgba(3, 169, 244, 0.14), transparent 42%),
        var(--bg);
      -webkit-font-smoothing: antialiased;
      -moz-osx-font-smoothing: grayscale;
      display: grid;
      grid-template-rows: auto 1fr auto;
    }
    .login-shell {
      display: grid;
      place-items: center;
      padding: 32px 20px;
    }
    .login-card {
      width: min(100%, 440px);
      padding: 36px;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 16px;
      box-shadow: var(--shadow-card);
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 22px;
    }
    .brand-mark {
      display: inline-grid;
      place-items: center;
      width: 44px;
      height: 44px;
      color: #ffffff;
      background: var(--gradient-brand);
      border-radius: 12px;
      box-shadow: var(--shadow-brand);
      font-size: 1.25rem;
    }
    .brand-meta { display: flex; flex-direction: column; gap: 2px; }
    .brand-name {
      color: var(--text-strong);
      font-size: 1.1rem;
      font-weight: 700;
      letter-spacing: -0.01em;
    }
    .brand-tag {
      color: var(--text-muted);
      font-size: 0.78rem;
      font-weight: 500;
    }
    h1 {
      margin: 0 0 8px;
      color: var(--text-strong);
      font-size: 1.5rem;
      font-weight: 700;
      letter-spacing: -0.02em;
    }
    .subtitle {
      margin: 0 0 24px;
      color: var(--text-muted);
      font-size: 0.9rem;
      line-height: 1.6;
    }
    .field { display: flex; flex-direction: column; gap: 6px; margin-bottom: 16px; }
    label {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      color: var(--text-strong);
      font-size: 0.85rem;
      font-weight: 600;
    }
    .input-wrap { position: relative; }
    .input-wrap > i {
      position: absolute;
      left: 12px;
      top: 50%;
      transform: translateY(-50%);
      color: var(--text-soft);
      pointer-events: none;
      font-size: 1rem;
    }
    input[type="password"] {
      width: 100%;
      min-height: 44px;
      padding: 9px 12px 9px 38px;
      color: var(--text);
      background: var(--surface);
      border: 1px solid var(--border-strong);
      border-radius: 8px;
      font-family: var(--font-family);
      font-size: 0.95rem;
      letter-spacing: 0.08em;
      outline: none;
      transition: border-color 180ms ease, box-shadow 180ms ease;
    }
    input[type="password"]::placeholder {
      color: var(--text-soft);
      letter-spacing: normal;
    }
    input[type="password"]:hover { border-color: #94a3b8; }
    input[type="password"]:focus {
      border-color: var(--primary);
      box-shadow: 0 0 0 3px rgba(67, 97, 238, 0.16);
    }
    button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      width: 100%;
      min-height: 44px;
      margin-top: 8px;
      padding: 10px 16px;
      color: #ffffff;
      background: var(--gradient-brand);
      border: 1px solid transparent;
      border-radius: 8px;
      cursor: pointer;
      font-family: var(--font-family);
      font-size: 0.95rem;
      font-weight: 600;
      box-shadow: var(--shadow-brand);
      transition: background 180ms ease, box-shadow 180ms ease, transform 120ms ease;
    }
    button:hover {
      background: var(--gradient-brand-hover);
      box-shadow: 0 8px 20px rgba(67, 97, 238, 0.32);
    }
    button:active { transform: translateY(1px); }
    .meta {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-top: 18px;
      padding: 10px 12px;
      color: #0369a1;
      background: rgba(3, 169, 244, 0.08);
      border: 1px solid rgba(3, 169, 244, 0.2);
      border-radius: 8px;
      font-size: 0.825rem;
    }
    .meta i { color: #03a9f4; font-size: 1rem; flex-shrink: 0; }
    .footer-line {
      padding: 18px 20px;
      color: var(--text-muted);
      font-size: 0.78rem;
      text-align: center;
    }
    @media (max-width: 480px) {
      .login-card { padding: 28px 22px; }
      h1 { font-size: 1.35rem; }
    }
  </style>
</head>
<body>
  <div></div>
  <main class="login-shell">
    <section class="login-card">
      <div class="brand">
        <span class="brand-mark"><i class="bi bi-journal-richtext"></i></span>
        <span class="brand-meta">
          <span class="brand-name">ScholarLens</span>
          <span class="brand-tag">学术文献智能搜索平台</span>
        </span>
      </div>
      <h1>欢迎回来</h1>
      <p class="subtitle">站点已启用临时访问保护，请输入访问密码继续使用搜索、历史记录与模型配置。</p>
      <form method="post" action="/login" autocomplete="on">
        <div class="field">
          <label for="password">访问密码</label>
          <div class="input-wrap">
            <i class="bi bi-shield-lock"></i>
            <input id="password" name="password" type="password" autocomplete="current-password" placeholder="请输入访问密码" autofocus required>
          </div>
        </div>
        <button type="submit"><i class="bi bi-box-arrow-in-right"></i> 进入 ScholarLens</button>
      </form>
      <div class="meta">
        <i class="bi bi-info-circle"></i>
        <span>会话将在 12 小时后自动失效，期间无需重新登录。</span>
      </div>
    </section>
  </main>
  <div class="footer-line">ScholarLens · 学术文献智能搜索平台</div>
</body>
</html>"#;

const LOGIN_FAILED_PAGE: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>密码错误 · ScholarLens</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap-icons@1.11.3/font/bootstrap-icons.min.css">
  <style>
    :root {
      color-scheme: light;
      --primary: #4361ee;
      --primary-dark: #3a0ca3;
      --gradient-brand: linear-gradient(135deg, #4361ee 0%, #3a0ca3 100%);
      --gradient-brand-hover: linear-gradient(135deg, #3a56d4 0%, #2f088b 100%);
      --bg: #f4f6fb;
      --surface: #ffffff;
      --border: #e4e7ee;
      --text-strong: #0f172a;
      --text-muted: #64748b;
      --danger: #ef4444;
      --danger-soft: rgba(239, 68, 68, 0.12);
      --shadow-brand: 0 6px 16px rgba(67, 97, 238, 0.22);
      --shadow-card: 0 12px 32px rgba(15, 23, 42, 0.10);
      --font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
        "Helvetica Neue", Arial, "PingFang SC", "Hiragino Sans GB",
        "Microsoft YaHei", "Noto Sans CJK SC", sans-serif;
    }
    * { box-sizing: border-box; }
    body {
      min-height: 100vh;
      margin: 0;
      display: grid;
      place-items: center;
      padding: 24px;
      color: var(--text-strong);
      font-family: var(--font-family);
      background:
        radial-gradient(circle at 14% 18%, rgba(239, 68, 68, 0.14), transparent 38%),
        radial-gradient(circle at 86% 12%, rgba(67, 97, 238, 0.16), transparent 42%),
        var(--bg);
    }
    .card {
      width: min(100%, 420px);
      padding: 32px 30px;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 16px;
      box-shadow: var(--shadow-card);
      text-align: center;
    }
    .icon {
      display: inline-grid;
      place-items: center;
      width: 56px;
      height: 56px;
      margin-bottom: 16px;
      color: var(--danger);
      background: var(--danger-soft);
      border-radius: 50%;
      font-size: 1.6rem;
    }
    h1 {
      margin: 0 0 8px;
      font-size: 1.3rem;
      font-weight: 700;
      letter-spacing: -0.01em;
    }
    p {
      margin: 0 0 22px;
      color: var(--text-muted);
      font-size: 0.9rem;
      line-height: 1.6;
    }
    a {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 10px 18px;
      color: #ffffff;
      text-decoration: none;
      background: var(--gradient-brand);
      border-radius: 8px;
      font-weight: 600;
      font-size: 0.9rem;
      box-shadow: var(--shadow-brand);
      transition: background 180ms ease, box-shadow 180ms ease, transform 120ms ease;
    }
    a:hover {
      background: var(--gradient-brand-hover);
      box-shadow: 0 8px 20px rgba(67, 97, 238, 0.32);
    }
    a:active { transform: translateY(1px); }
  </style>
</head>
<body>
  <main class="card">
    <div class="icon"><i class="bi bi-exclamation-triangle"></i></div>
    <h1>密码错误</h1>
    <p>访问密码不正确，请重新输入临时访问密码以继续使用 ScholarLens。</p>
    <a href="/login"><i class="bi bi-arrow-left"></i> 重新登录</a>
  </main>
</body>
</html>"#;
