//! 认证流程：CAS 密码 + 图片验证码 + 企业微信验证码，或扫码登录；
//! 随后完成 iLearn JSONP 登录、SSO 交换、ilearntec 业务会话和资源中心会话。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;
use reqwest::{Client, Response};
use serde_json::Value;
use url::Url;

use crate::api::ApiClient;
use crate::models::{SessionFile, User};
use crate::util::{
    detect_image_ext, extract_input, extract_jsonp_field, extract_proxy_credentials, find_ticket,
    has_captcha_error, has_login_error, is_second_factor_page, now_ms, open_file,
    safe_truncate, url_encode,
};

pub const CAS_BASE: &str = "https://cas.jlu.edu.cn";
pub const CAS_SERVICE: &str =
    "https://jwcidentity.jlu.edu.cn/iplat-pass-jlu/thirdLogin/jlu/login";
pub const ILEARN_BASE: &str = "https://ilearn.jlu.edu.cn";
pub const ILEARNTEC_SERVICE: &str = "https://ilearntec.jlu.edu.cn/";
pub const COURSECENTER_SERVICE: &str =
    "https://ilearntec.jlu.edu.cn/coursecenter/main/index";

/// 用多个深路径 URL 采集 Cookie，避免遗漏 Path=/tpass、/cas-server、/coursecenter 等。
const COOKIE_URLS: &[&str] = &[
    "https://cas.jlu.edu.cn/tpass/login",
    "https://cas.jlu.edu.cn/",
    "https://jwcidentity.jlu.edu.cn/iplat-pass-jlu/thirdLogin/jlu/login",
    "https://ilearn.jlu.edu.cn/cas-server/login",
    "https://ilearn.jlu.edu.cn/iplat/ssoservice",
    "https://ilearn.jlu.edu.cn/",
    "https://ilearntec.jlu.edu.cn/coursecenter/main/index",
    "https://ilearntec.jlu.edu.cn/",
    "https://ilearnres.jlu.edu.cn/resource-center/user/index",
    "https://ilearnres.jlu.edu.cn/resource-center/user/info",
    "https://ilearnres.jlu.edu.cn/",
];

#[derive(Debug, Clone)]
pub struct LoginForm {
    pub lt: String,
    pub execution: String,
}

pub enum PasswordOutcome {
    NeedSecondFactor { lt: String, execution: String },
    Complete(Session),
}

#[derive(Debug)]
pub enum WxCodeStatus {
    Sent,
    Unknown,
}

pub enum QrPoll {
    Pending,
    Expired,
    Scanned(Session),
}

pub struct Session {
    pub client: Client,
    pub user: User,
}

pub struct AuthClient {
    http: Client,
    follow: Client,
    jar: Arc<reqwest::cookie::Jar>,
    cache_dir: PathBuf,
}

impl AuthClient {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let cache_dir = cache_dir.into();
        std::fs::create_dir_all(&cache_dir)?;

        let jar = Arc::new(reqwest::cookie::Jar::default());
        let http = build_client(jar.clone(), false)?;
        let follow = build_client(jar.clone(), true)?;

        Ok(Self {
            http,
            follow,
            jar,
            cache_dir,
        })
    }

    // ── 会话缓存 ─────────────────────────────────────────────

    /// 清空内存中的 Cookie 会话。
    pub fn jar(&self) -> Arc<reqwest::cookie::Jar> {
        self.jar.clone()
    }

    pub fn reset(&mut self) -> Result<()> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        self.http = build_client(jar.clone(), false)?;
        self.follow = build_client(jar.clone(), true)?;
        self.jar = jar;
        Ok(())
    }

    pub fn load_session(&self, path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        let text = std::fs::read_to_string(path)?;
        let file: SessionFile = serde_json::from_str(&text)?;
        for (domain, cookies) in &file.cookies {
            let url = Url::parse(&format!("https://{domain}/"))?;
            for cookie in cookies {
                self.jar.add_cookie_str(cookie, &url);
            }
        }
        log::info!("已加载本地会话缓存: {}", path.display());
        Ok(true)
    }

    pub fn save_session(&self, path: &Path) -> Result<()> {
        let mut by_domain: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for raw in COOKIE_URLS {
            let url = Url::parse(raw)?;
            let domain = match url.host_str() {
                Some(d) => d.to_string(),
                None => continue,
            };
            if let Some(header) = self.jar.cookies(&url) {
                if let Ok(s) = header.to_str() {
                    for pair in s.split(';').map(str::trim) {
                        if !pair.is_empty() && pair.contains('=') {
                            by_domain
                                .entry(domain.clone())
                                .or_default()
                                .insert(pair.to_string());
                        }
                    }
                }
            }
        }
        let map = by_domain
            .into_iter()
            .map(|(domain, set)| (domain, set.into_iter().collect()))
            .collect();
        let file = SessionFile {
            saved_at_ms: now_ms(),
            cookies: map,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&file)?)?;
        log::info!("已保存本地会话缓存: {}", path.display());
        Ok(())
    }

    pub async fn cached_session(&self) -> Option<Session> {
        let api = ApiClient::new(self.follow.clone());
        match api.user_mid().await {
            Ok(user) => {
                log::info!("缓存会话仍然有效: {}", user.username);
                Some(Session {
                    client: self.follow.clone(),
                    user,
                })
            }
            Err(e) => {
                log::info!("缓存会话不可用: {e}");
                None
            }
        }
    }

    // ── Phase 1：CAS 密码 ───────────────────────────────────

    pub async fn fetch_login_form(&self) -> Result<LoginForm> {
        let url = format!("{CAS_BASE}/tpass/login?service={}", url_encode(CAS_SERVICE));
        log::info!("GET {url}");
        let resp = self
            .http
            .get(&url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .send()
            .await
            .context("GET CAS 登录页")?;
        let status = resp.status();
        let html = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("CAS 登录页 HTTP {status}"));
        }
        let lt = extract_input(&html, "lt").ok_or_else(|| anyhow!("登录页未找到 lt"))?;
        let execution =
            extract_input(&html, "execution").ok_or_else(|| anyhow!("登录页未找到 execution"))?;
        Ok(LoginForm { lt, execution })
    }

    pub async fn submit_password(
        &self,
        form: &LoginForm,
        username: &str,
        password: &str,
    ) -> Result<PasswordOutcome> {
        let rsa = crate::des::des_init(username, password, &form.lt);
        let url = format!("{CAS_BASE}/tpass/login?service={}", url_encode(CAS_SERVICE));
        log::info!("POST {url} (密码提交)");
        let ul = username.chars().count().to_string();
        let pl = password.chars().count().to_string();

        let resp = self
            .http
            .post(&url)
            .header("Referer", &url)
            .header("Origin", CAS_BASE)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .form(&[
                ("rsa", rsa.as_str()),
                ("ul", ul.as_str()),
                ("pl", pl.as_str()),
                ("sl", "0"),
                ("lt", form.lt.as_str()),
                ("execution", form.execution.as_str()),
                ("_eventId", "submit"),
            ])
            .send()
            .await
            .context("POST CAS 密码")?;

        let status = resp.status();
        if status.is_redirection() {
            let location = header_location(&resp)?;
            log::info!("密码提交直接 302 → {}", redact_location(&location));
            return Ok(PasswordOutcome::Complete(
                self.complete_from_jwcidentity(&location).await?,
            ));
        }
        if !status.is_success() {
            return Err(anyhow!("密码提交 HTTP {status}"));
        }

        let html = resp.text().await?;
        if has_login_error(&html) {
            return Err(anyhow!("用户名或密码错误"));
        }
        if is_second_factor_page(&html) {
            let lt = extract_input(&html, "lt").unwrap_or_else(|| form.lt.clone());
            let execution = extract_input(&html, "execution")
                .ok_or_else(|| anyhow!("二次验证页未找到 execution"))?;
            log::info!("进入二次验证页: execution={execution}");
            return Ok(PasswordOutcome::NeedSecondFactor { lt, execution });
        }

        Err(anyhow!(
            "密码提交返回 200，但无法识别页面（前 200 字）：{}",
            safe_truncate(&html, 200)
        ))
    }

    pub async fn fetch_captcha(&self) -> Result<PathBuf> {
        let url = format!("{CAS_BASE}/tpass/code");
        log::info!("GET {url}");
        let resp = self
            .http
            .get(&url)
            .header("Referer", format!("{CAS_BASE}/tpass/login"))
            .send()
            .await
            .context("GET 图片验证码")?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() || bytes.is_empty() {
            return Err(anyhow!("图片验证码获取失败: HTTP {status}, {} bytes", bytes.len()));
        }
        let path = self
            .cache_dir
            .join(format!("captcha.{}", detect_image_ext(&bytes)));
        std::fs::write(&path, &bytes)?;
        log::info!("验证码已保存: {}", path.display());
        Ok(path)
    }

    pub async fn request_wx_code(&self, captcha: &str) -> Result<WxCodeStatus> {
        let url = format!(
            "{CAS_BASE}/tpass/recheckcode?code={}&t={}",
            url_encode(captcha),
            now_ms()
        );
        log::info!("GET {url}");
        let resp = self
            .http
            .get(&url)
            .header("Referer", format!("{CAS_BASE}/tpass/login"))
            .send()
            .await
            .context("请求企业微信验证码")?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("企业微信验证码请求 HTTP {status}"));
        }

        let cookie_status = resp
            .cookies()
            .find(|c| c.name() == "recheck_mobile_error_info")
            .map(|c| c.value().to_string());

        match cookie_status.as_deref() {
            Some("success") => Ok(WxCodeStatus::Sent),
            Some("img_code_error") => Err(anyhow!("图片验证码错误")),
            Some("user_not_exist") => Err(anyhow!("该账号未绑定手机号")),
            Some("get_times_more") => Err(anyhow!("请勿重复获取验证码")),
            Some("fail") => Err(anyhow!("企业微信验证码发送失败")),
            Some(other) => Err(anyhow!("企业微信验证码状态未知: {other}")),
            None => {
                // 某些版本不通过 Cookie 返回状态；200 + 空 body 时按已发送处理。
                log::warn!("未读取到 recheck_mobile_error_info，按“已发送”继续");
                Ok(WxCodeStatus::Unknown)
            }
        }
    }

    pub async fn submit_verification(
        &self,
        lt: &str,
        execution: &str,
        captcha: &str,
        wx_code: &str,
    ) -> Result<Session> {
        let url = format!("{CAS_BASE}/tpass/login?service={}", url_encode(CAS_SERVICE));
        log::info!("POST {url} (双验证码提交)");
        let resp = self
            .http
            .post(&url)
            .header("Referer", &url)
            .header("Origin", CAS_BASE)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .form(&[
                ("code", captcha),
                ("WxCode", wx_code),
                ("rsa", ""),
                ("ul", ""),
                ("pl", ""),
                ("sl", ""),
                ("lt", lt),
                ("execution", execution),
                ("_eventId", "submit"),
            ])
            .send()
            .await
            .context("POST 双验证码")?;

        let status = resp.status();
        if status.is_redirection() {
            let location = header_location(&resp)?;
            log::info!("双验证码 302 → {}", redact_location(&location));
            return self.complete_from_jwcidentity(&location).await;
        }

        let html = resp.text().await?;
        if has_captcha_error(&html) {
            return Err(anyhow!("验证码错误，请重新获取图片验证码"));
        }
        Err(anyhow!(
            "双验证码提交 HTTP {status}，页面无法识别：{}",
            safe_truncate(&html, 200)
        ))
    }

    // ── 扫码登录 ─────────────────────────────────────────────

    /// 生成二维码，返回 (uuid, png 路径)。GUI 可直接读取 PNG。
    pub async fn generate_qr(&self) -> Result<(String, PathBuf)> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let qr_url = format!(
            "{CAS_BASE}/tpass/qyQrLogin?uuid={uuid}&service={}",
            url_encode(CAS_SERVICE)
        );

        let code = qrcode::QrCode::new(qr_url.as_bytes())
            .map_err(|e| anyhow!("生成二维码失败: {e}"))?;
        let image = code.render::<image::Luma<u8>>().build();
        let path = self.cache_dir.join("qrcode.png");
        image.save(&path)?;
        Ok((uuid, path))
    }

    /// 轮询一次扫码状态。扫码成功后完成后续 SSO。
    pub async fn poll_qr(&self, uuid: &str) -> Result<QrPoll> {
        let random = (now_ms() % 100000) as f64 / 100.0;
        let url = format!("{CAS_BASE}/tpass/checkQRCodeScan?random={random}&uuid={uuid}");
        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest")
            .send()
            .await
            .context("轮询扫码状态")?;
        let body = resp.text().await?;
        if body.trim().is_empty() {
            return Ok(QrPoll::Pending);
        }

        if let Some(json) = serde_json::from_str::<Value>(&body).ok() {
            if json.get("redirect_url").is_some() || json.get("redirectUrl").is_some() {
                return Ok(QrPoll::Scanned(self.finish_qr_login().await?));
            }
            let code = json
                .get("code")
                .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
            if matches!(code, Some(0) | Some(2)) {
                return Ok(QrPoll::Expired);
            }
            if matches!(code, Some(1))
                || json.get("status").is_some()
                || json.get("success").is_some()
                || body.contains("success")
                || body.contains("scanned")
            {
                return Ok(QrPoll::Scanned(self.finish_qr_login().await?));
            }
        }
        Ok(QrPoll::Pending)
    }

    async fn finish_qr_login(&self) -> Result<Session> {
        // 浏览器扫码成功后会重新加载 CAS 登录页，由 CAS 302 到 jwcidentity。
        let login_url = format!("{CAS_BASE}/tpass/login?service={}", url_encode(CAS_SERVICE));
        let resp = self
            .follow
            .get(&login_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("扫码后加载 CAS 登录页")?;
        let status = resp.status();
        let final_url = resp.url().to_string();
        let html = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("扫码后 CAS 返回 HTTP {status}"));
        }
        log::info!("扫码后落地页: {}", redact_location(&final_url));

        let (user, pass) = extract_proxy_credentials(&html).ok_or_else(|| {
            anyhow!(
                "扫码后未找到代理凭据（落地页 {}，HTML {} 字节）",
                redact_location(&final_url),
                html.len()
            )
        })?;
        self.sso_exchange(&user, &pass).await
    }

    /// CLI 用的一体化扫码登录：生成、打开、轮询。
    pub async fn qr_login(&self) -> Result<Session> {
        let (uuid, path) = self.generate_qr().await?;
        println!("请使用企业微信扫描二维码：{}", path.display());
        open_file(&path);

        for attempt in 1..=60 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            match self.poll_qr(&uuid).await? {
                QrPoll::Pending => {
                    if attempt % 5 == 0 {
                        println!("等待扫码... ({attempt}/60)");
                    }
                }
                QrPoll::Expired => return Err(anyhow!("二维码已失效，请重新运行")),
                QrPoll::Scanned(session) => return Ok(session),
            }
        }
        Err(anyhow!("二维码等待超时"))
    }

    // ── Phase 1.5 / 2 / 3 ───────────────────────────────────

    async fn complete_from_jwcidentity(&self, initial_url: &str) -> Result<Session> {
        log::info!("跟随 jwcidentity 链: {}", redact_location(initial_url));
        let resp = self
            .follow
            .get(initial_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("跟随 jwcidentity 重定向")?;
        let status = resp.status();
        let final_url = resp.url().to_string();
        let html = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("jwcidentity 最终页 HTTP {status}"));
        }

        if let Some((user, pass)) = extract_proxy_credentials(&html) {
            log::info!("获取到代理凭据: {user}");
            return self.sso_exchange(&user, &pass).await;
        }

        // 少数情况下 302 已经直接建立了 ilearntec 会话，先验证一次。
        let api = ApiClient::new(self.follow.clone());
        if let Ok(user) = api.user_mid().await {
            log::info!("jwcidentity 链后已是登录态: {}", user.username);
            return Ok(Session {
                client: self.follow.clone(),
                user,
            });
        }

        Err(anyhow!(
            "未从 jwcidentity 页面提取到代理凭据（最终 URL {}，HTML {} 字节）",
            redact_location(&final_url),
            html.len()
        ))
    }

    async fn sso_exchange(&self, proxy_user: &str, proxy_pass: &str) -> Result<Session> {
        log::info!("开始 iLearn SSO 交换: {proxy_user}");

        // 1. 退出旧 iLearn CAS 会话
        let logout = format!(
            "{ILEARN_BASE}/cas-server/logout?callback=logoutcallback&_={}",
            now_ms()
        );
        let _ = self.follow.get(&logout).send().await;

        // 2. JSONP get-lt
        let ts = now_ms();
        let getlt_url = format!(
            "{ILEARN_BASE}/cas-server/login?service={}&get-lt=true&callback=jsonpcallback&n={}&_={}",
            url_encode(ILEARNTEC_SERVICE),
            ts + 1,
            ts
        );
        let resp = self
            .http
            .get(&getlt_url)
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .send()
            .await
            .context("iLearn get-lt")?;
        let body = resp.text().await?;
        let lt = extract_jsonp_field(&body, "lt")
            .ok_or_else(|| anyhow!("iLearn get-lt 未返回 lt: {}", safe_truncate(&body, 200)))?;
        let execution = extract_jsonp_field(&body, "execution")
            .ok_or_else(|| anyhow!("iLearn get-lt 未返回 execution"))?;
        log::info!("iLearn lt 长度={}, execution={}", lt.len(), execution);

        // 3. JSONP 登录
        use base64::Engine as _;
        let pwd_b64 = base64::engine::general_purpose::STANDARD.encode(proxy_pass.as_bytes());
        let ts2 = now_ms();
        let login_url = format!(
            "{ILEARN_BASE}/cas-server/login?service={}&username={}&password={}&callback=logincallback&lt={}&execution={}&n={}&isajax=true&isframe=true&_eventId=submit&_={}&serviceURL=&type=pwd",
            url_encode(ILEARNTEC_SERVICE),
            url_encode(proxy_user),
            url_encode(&pwd_b64),
            url_encode(&lt),
            url_encode(&execution),
            ts2 + 1,
            ts2,
        );
        let resp = self
            .http
            .get(&login_url)
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .send()
            .await
            .context("iLearn JSONP 登录")?;
        let body = resp.text().await?;
        let ticket = find_ticket(&body)
            .ok_or_else(|| anyhow!("iLearn 登录未返回 ticket: {}", safe_truncate(&body, 300)))?;
        log::info!("iLearn ticket 长度={}", ticket.len());

        // 4. SSO 交换
        let sso_url = format!(
            "{ILEARN_BASE}/iplat/ssoservice?ssoservice={}",
            url_encode(ILEARNTEC_SERVICE)
        );
        let resp = self
            .follow
            .get(&sso_url)
            .header("Referer", format!("{ILEARN_BASE}/"))
            .header("Origin", ILEARN_BASE)
            .send()
            .await
            .context("iLearn SSO 交换")?;
        let sso_status = resp.status();
        log::info!("SSO 交换结束: HTTP {sso_status}");
        if !sso_status.is_success() {
            let alt = format!("{sso_url}&ticket={}", url_encode(&ticket));
            let resp = self.follow.get(&alt).send().await?;
            if !resp.status().is_success() {
                return Err(anyhow!("SSO 交换失败: HTTP {}", resp.status()));
            }
        }

        // 5. 关键：第二次 CAS 交换，service=coursecenter
        let cc_login = format!(
            "{ILEARN_BASE}/cas-server/login?service={}",
            url_encode(COURSECENTER_SERVICE)
        );
        let resp = self
            .follow
            .get(&cc_login)
            .header("Referer", format!("{ILEARN_BASE}/"))
            .send()
            .await
            .context("coursecenter CAS 交换")?;
        let cc_status = resp.status();
        let cc_final = resp.url().to_string();
        log::info!(
            "coursecenter CAS 结束: HTTP {}，最终 URL {}",
            cc_status,
            redact_location(&cc_final)
        );
        if !cc_status.is_success() {
            return Err(anyhow!("coursecenter CAS 交换失败: HTTP {cc_status}"));
        }

        // 6. 登录态门禁：getUserByMid == 1000
        let api = ApiClient::new(self.follow.clone());
        let user = api
            .user_mid()
            .await
            .context("登录态校验失败（getUserByMid != 1000）")?;
        log::info!("登录成功: {} / {}", user.username, user.realname);

        // 7. 资源中心会话（失败不阻塞课程信息，但视频会不可用）
        if let Err(e) = api.ensure_res_session().await {
            log::warn!("资源中心会话建立失败，视频信息可能不可用: {e:#}");
        } else {
            log::info!("资源中心会话已建立");
        }

        Ok(Session {
            client: self.follow.clone(),
            user,
        })
    }

}

fn build_client(jar: Arc<reqwest::cookie::Jar>, follow: bool) -> Result<Client> {
    let mut builder = Client::builder()
        .cookie_provider(jar)
        .timeout(Duration::from_secs(30))
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::USER_AGENT,
                HeaderValue::from_static(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                ),
            );
            h.insert(
                reqwest::header::ACCEPT_LANGUAGE,
                HeaderValue::from_static("zh-CN,zh;q=0.9"),
            );
            h
        });
    if !follow {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    Ok(builder.build()?)
}

fn header_location(resp: &Response) -> Result<String> {
    resp.headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("响应缺少 Location"))
}

/// 只隐藏 Location 中的 ticket，保留 service 和路径，便于日志排查。
fn redact_location(loc: &str) -> String {
    match Url::parse(loc) {
        Ok(mut u) => {
            let pairs: Vec<(String, String)> = u
                .query_pairs()
                .map(|(k, v)| {
                    if k.eq_ignore_ascii_case("ticket") {
                        (k.to_string(), "***".to_string())
                    } else {
                        (k.to_string(), v.to_string())
                    }
                })
                .collect();
            if !pairs.is_empty() {
                u.query_pairs_mut().clear();
                for (k, v) in pairs {
                    u.query_pairs_mut().append_pair(&k, &v);
                }
            }
            let s = u.to_string();
            safe_truncate(&s, 300).to_string()
        }
        Err(_) => safe_truncate(loc, 300).to_string(),
    }
}
