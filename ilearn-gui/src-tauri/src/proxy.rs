//! 本地媒体代理。
//!
//! WebView 直接访问 jwcmedia 的签名 URL 时，部分环境下会缺少 Cookie/Referer，
//! 或者被跨域策略拦截。这里用 127.0.0.1 上的本地代理转发请求：
//! - 复用登录后的 Cookie Jar
//! - 固定 Referer / Origin
//! - 原样转发 Range，支持 206 分段播放
//! - 把 application/octet-stream 修正为 video/mp4

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use reqwest::Client;
use url::Url;

#[derive(Clone)]
struct ProxyState {
    client: Client,
}

pub fn spawn(jar: Arc<reqwest::cookie::Jar>) -> anyhow::Result<String> {
    let client = Client::builder()
        .cookie_provider(jar)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let listener = tauri::async_runtime::block_on(async {
        tokio::net::TcpListener::bind(("127.0.0.1", 0)).await
    })?;
    let addr = listener.local_addr()?;

    let app = Router::new()
        .route("/media", get(media))
        .with_state(ProxyState { client });

    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("媒体代理退出: {e:#}");
        }
    });

    Ok(format!("http://127.0.0.1:{}", addr.port()))
}

fn allowed(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.ends_with("jlu.edu.cn") || host.ends_with("fifedu.com")
}

async fn media(
    State(state): State<ProxyState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let Some(raw) = params.get("url") else {
        return (StatusCode::BAD_REQUEST, "missing url").into_response();
    };
    let Ok(target) = Url::parse(raw) else {
        return (StatusCode::BAD_REQUEST, "invalid url").into_response();
    };
    if !allowed(&target) {
        return (StatusCode::FORBIDDEN, "host not allowed").into_response();
    }

    let mut req = state
        .client
        .get(target.clone())
        .header(
            header::REFERER,
            "https://ilearntec.jlu.edu.cn/",
        )
        .header(header::ORIGIN, "https://ilearntec.jlu.edu.cn")
        .header(
            header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header(header::ACCEPT, "*/*");

    if let Some(range) = headers.get(header::RANGE) {
        req = req.header(header::RANGE, range);
    }
    if let Some(if_range) = headers.get(header::IF_RANGE) {
        req = req.header(header::IF_RANGE, if_range);
    }

    let upstream = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("媒体代理上游请求失败: {e:#}");
            return (StatusCode::BAD_GATEWAY, "upstream error").into_response();
        }
    };

    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();

    let mut builder = Response::builder().status(status);
    for name in [
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::ETAG,
        header::LAST_MODIFIED,
        header::CACHE_CONTROL,
    ] {
        if let Some(value) = upstream_headers.get(&name) {
            builder = builder.header(name, value);
        }
    }

    // 只设置一次 Content-Type：部分 jwcmedia 响应是 octet-stream，
    // WebView 用它判断媒体类型会不积极，这里修正为 video/mp4。
    let content_type = upstream_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.starts_with("application/octet-stream"))
        .unwrap_or("video/mp4");
    builder = builder.header(header::CONTENT_TYPE, content_type);

    builder = builder
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Range, Accept-Ranges, Content-Length, Content-Type",
        )
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS");

    let body = Body::from_stream(upstream.bytes_stream());
    builder.body(body).unwrap_or_else(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "build response failed").into_response()
    })
}
