//! 学在吉大业务 API 客户端。

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::Value;

use crate::models::{Lesson, Record, Term, User, VideoResource, WeekList};

pub const BASE: &str = "https://ilearntec.jlu.edu.cn";
pub const RES_BASE: &str = "https://ilearnres.jlu.edu.cn";

pub struct ApiClient {
    client: Client,
}

#[derive(Debug, serde::Deserialize)]
struct Envelope {
    code: Option<Value>,
    status: Option<Value>,
    message: Option<String>,
    data: Option<Value>,
    success: Option<bool>,
}

fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

impl Envelope {
    fn code(&self) -> Option<String> {
        self.code.as_ref().and_then(value_to_string)
    }

    fn status(&self) -> Option<String> {
        self.status.as_ref().and_then(value_to_string)
    }

    fn ok(&self) -> bool {
        self.code().as_deref() == Some("1000")
            || self.code().as_deref() == Some("200")
            || self.status().as_deref() == Some("1")
            || self.success == Some(true)
    }

    fn error_text(&self) -> String {
        self.message
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "code={:?}, status={:?}",
                    self.code(),
                    self.status()
                )
            })
    }
}

impl ApiClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    async fn get_json(&self, url: &str) -> Result<Envelope> {
        let resp = self
            .client
            .get(url)
            .header("Accept", "application/json, text/plain, */*")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("GET {url} 返回 HTTP {status}: {}", crate::util::safe_truncate(&text, 300)));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("解析 {url} 响应失败: {}", crate::util::safe_truncate(&text, 300)))
    }

    async fn post_json(&self, url: &str) -> Result<Envelope> {
        let resp = self
            .client
            .post(url)
            .header("Accept", "application/json, text/plain, */*")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("POST {url} 返回 HTTP {status}: {}", crate::util::safe_truncate(&text, 300)));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("解析 {url} 响应失败: {}", crate::util::safe_truncate(&text, 300)))
    }

    /// 登录态唯一判据：`getUserByMid` 的 code == "1000"。
    pub async fn user_mid(&self) -> Result<User> {
        let url = format!("{BASE}/coursecenter/iplate/getUserByMid");
        let env = self.get_json(&url).await?;
        if !env.ok() {
            return Err(anyhow!("未登录: {}", env.error_text()));
        }
        let data = env.data.ok_or_else(|| anyhow!("getUserByMid 缺少 data"))?;
        let item = data
            .get("dataList")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow!("getUserByMid 的 data.dataList 为空"))?;
        serde_json::from_value(item.clone()).context("解析 User 失败")
    }

    pub async fn terms(&self) -> Result<Vec<Term>> {
        let url = format!("{BASE}/studycenter/platform/common/termList");
        let env = self.post_json(&url).await?;
        if !env.ok() {
            return Err(anyhow!("获取学期失败: {}", env.error_text()));
        }
        let data = env.data.ok_or_else(|| anyhow!("termList 缺少 data"))?;
        let list = data
            .get("dataList")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(list)).context("解析 Term 列表失败")
    }

    pub async fn lessons(&self, term_year: &str, term: &str) -> Result<Vec<Lesson>> {
        let url = format!(
            "{BASE}/studycenter/platform/classroom/myClassroom?termYear={}&term={}",
            crate::util::url_encode(term_year),
            crate::util::url_encode(term)
        );
        let env = self.get_json(&url).await?;
        if !env.ok() {
            return Err(anyhow!("获取课程失败: {}", env.error_text()));
        }
        let data = env.data.ok_or_else(|| anyhow!("myClassroom 缺少 data"))?;
        let list = data
            .get("dataList")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(list)).context("解析 Lesson 列表失败")
    }

    pub async fn weeks(&self, term_id: &str) -> Result<WeekList> {
        let url = format!(
            "{BASE}/coursecenter/liveAndRecord/getWeekList?identity=2&termId={}",
            crate::util::url_encode(term_id)
        );
        let env = self.get_json(&url).await?;
        if !env.ok() {
            return Err(anyhow!("获取周列表失败: {}", env.error_text()));
        }
        let data = env.data.ok_or_else(|| anyhow!("getWeekList 缺少 data"))?;
        serde_json::from_value(data).context("解析 WeekList 失败")
    }

    pub async fn records(&self, term_id: &str, teach_class_id: &str) -> Result<Vec<Record>> {
        let url = format!("{BASE}/coursecenter/liveAndRecord/getLiveAndRecordInfoList");
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("memberId", ""),
                ("termId", term_id),
                ("roomType", "0"),
                ("identity", "2"),
                ("liveStatus", "0"),
                ("submitStatus", "0"),
                ("weekNum", ""),
                ("dayNum", ""),
                ("timeRange", ""),
                ("teachClassId", teach_class_id),
            ])
            .header("Accept", "application/json, text/plain, */*")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("GET getLiveAndRecordInfoList")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!(
                "获取录播失败 HTTP {status}: {}",
                crate::util::safe_truncate(&text, 300)
            ));
        }
        let env: Envelope = serde_json::from_str(&text)
            .with_context(|| format!("解析录播响应失败: {}", crate::util::safe_truncate(&text, 300)))?;
        if !env.ok() {
            return Err(anyhow!("获取录播失败: {}", env.error_text()));
        }
        let data = env.data.ok_or_else(|| anyhow!("录播响应缺少 data"))?;
        let list = data
            .get("dataList")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        serde_json::from_value(Value::Array(list)).context("解析 Record 列表失败")
    }

    /// 让资源中心建立独立 CAS 会话。必须使用带登录 Cookie 且会跟随重定向的客户端。
    pub async fn ensure_res_session(&self) -> Result<()> {
        let index = format!("{RES_BASE}/resource-center/user/index");
        let resp = self
            .client
            .get(&index)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await
            .context("GET resource-center/user/index")?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("资源中心入口返回 HTTP {status}"));
        }

        let info = format!("{RES_BASE}/resource-center/user/info");
        let env = self.get_json(&info).await?;
        if !env.ok() {
            return Err(anyhow!("资源中心未登录: {}", env.error_text()));
        }
        Ok(())
    }

    pub async fn video_resource(&self, resource_id: &str) -> Result<VideoResource> {
        let url = format!(
            "{RES_BASE}/resource-center/videoclass/videoClassInfo?resourceId={}",
            crate::util::url_encode(resource_id)
        );
        let env = self.get_json(&url).await?;
        if !env.ok() {
            return Err(anyhow!("获取视频信息失败: {}", env.error_text()));
        }
        let data = env.data.ok_or_else(|| anyhow!("videoClassInfo 缺少 data"))?;
        let resource: VideoResource =
            serde_json::from_value(data).context("解析 VideoResource 失败")?;
        if resource.video_list.is_empty() {
            return Err(anyhow!("videoClassInfo 的 videoList 为空"));
        }
        Ok(resource)
    }

    /// 下载任意 URL（用于 `videoClassInfo.phaseUrl` 这类带签名的字幕地址）。
    pub async fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .header("Accept", "*/*")
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("下载失败 HTTP {status}: {url}"));
        }
        if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if ct.to_str().unwrap_or("").contains("text/html") {
                return Err(anyhow!("下载返回 HTML，可能未登录: {url}"));
            }
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// 资源中心字幕下载接口（VTT/SRT 以实际响应为准）。
    pub async fn subtitle(&self, resource_id: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{RES_BASE}/resource-center/word/downLoadCaption?resourceId={}",
            crate::util::url_encode(resource_id)
        );
        let resp = self
            .client
            .get(&url)
            .header("Accept", "*/*")
            .send()
            .await
            .context("下载字幕")?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("字幕下载 HTTP {status}"));
        }
        if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if ct.to_str().unwrap_or("").contains("text/html") {
                return Err(anyhow!("字幕接口返回 HTML，可能未登录或接口已变化"));
            }
        }
        Ok(resp.bytes().await?.to_vec())
    }
}
