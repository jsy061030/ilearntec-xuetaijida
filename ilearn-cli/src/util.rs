//! 通用工具：URL 编码、JSONP 解析、HTML 表单解析、终端输入。

use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 对 query 参数值做标准 percent-encoding（会正确编码 `+`、`/`、`=`）。
pub fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// 从 JSONP / 纯 JSON 文本中提取 JSON 对象。
pub fn parse_jsonp(body: &str) -> Option<Value> {
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&body[start..=end]).ok()
}

pub fn extract_jsonp_field(body: &str, field: &str) -> Option<String> {
    let json = parse_jsonp(body)?;
    json.get(field).and_then(|v| v.as_str()).map(str::to_string)
}

/// 递归查找 `ticket` 字段，适配不同 JSONP 包装。
pub fn find_ticket(body: &str) -> Option<String> {
    fn walk(v: &Value) -> Option<String> {
        match v {
            Value::Object(map) => {
                for (k, val) in map {
                    if k == "ticket" {
                        if let Some(s) = val.as_str() {
                            return Some(s.to_string());
                        }
                    }
                    if let Some(found) = walk(val) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(arr) => arr.iter().find_map(walk),
            _ => None,
        }
    }
    walk(&parse_jsonp(body)?)
}

/// 从 HTML 的 `<input name="...">` 读取 value。
pub fn extract_input(html: &str, name: &str) -> Option<String> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let sel = Selector::parse(&format!("input[name=\"{}\"]", name)).ok()?;
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// 从 jwcidentity 页面提取代理用户名/密码。
/// 优先使用 `#username` / `#password`，其次兼容 `EduSSO.login(...)`。
pub fn extract_proxy_credentials(html: &str) -> Option<(String, String)> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let user = doc
        .select(&Selector::parse("#username").ok()?)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string);
    let pass = doc
        .select(&Selector::parse("#password").ok()?)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string);

    if let (Some(u), Some(p)) = (user, pass) {
        if !u.is_empty() && !p.is_empty() {
            return Some((u, p));
        }
    }

    let re = regex::Regex::new(
        r#"login\(\s*"[^"]*"\s*,\s*"[^"]*"\s*,\s*"[^"]*"\s*,\s*"([^"]+)"\s*,\s*"([^"]+)"\s*[,)]"#,
    )
    .ok()?;
    let caps = re.captures(html)?;
    let u = caps.get(1)?.as_str().to_string();
    let p = caps.get(2)?.as_str().to_string();
    if u.is_empty() || p.is_empty() {
        None
    } else {
        Some((u, p))
    }
}

/// 判断 HTML 是否是二次验证页。注意：初始登录页也可能含 codeImage，
/// 不能只用 codeImage 判断。
pub fn is_second_factor_page(html: &str) -> bool {
    html.contains("getWxCode")
        || html.contains("toRecheckLogin")
        || html.contains("recheckcode")
}

pub fn has_login_error(html: &str) -> bool {
    html.contains("用户名密码错误")
        || html.contains("用户名或密码错误")
        || html.contains("密码错误")
        || html.contains("账号或密码错误")
}

pub fn has_captcha_error(html: &str) -> bool {
    html.contains("验证码错误") || html.contains("验证码不正确") || html.contains("图形验证码错误")
}

pub fn detect_image_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        "jpg"
    } else {
        "png"
    }
}

pub fn open_file(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(path).spawn();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}

pub fn prompt(label: &str) -> io::Result<String> {
    print!("{}", label);
    io::stdout().flush()?;
    let mut line = String::new();
    let n = io::stdin().read_line(&mut line)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "标准输入已关闭，无法继续交互",
        ));
    }
    Ok(line.trim().to_string())
}

pub fn prompt_required(label: &str) -> io::Result<String> {
    loop {
        let v = prompt(label)?;
        if !v.is_empty() {
            return Ok(v);
        }
        eprintln!("输入不能为空，请重试。");
    }
}

pub fn prompt_password(label: &str) -> io::Result<String> {
    rpassword::prompt_password(label)
}

pub fn prompt_select(label: &str, items: &[String]) -> io::Result<usize> {
    if items.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "没有可选项"));
    }
    println!("\n{}", label);
    for (i, item) in items.iter().enumerate() {
        println!("  [{}] {}", i + 1, item);
    }
    loop {
        let raw = prompt(&format!("请选择 [1-{}]，直接回车选 1：", items.len()))?;
        let n = if raw.is_empty() {
            1
        } else {
            match raw.parse::<usize>() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("请输入数字。");
                    continue;
                }
            }
        };
        if (1..=items.len()).contains(&n) {
            return Ok(n - 1);
        }
        eprintln!("范围应为 1-{}。", items.len());
    }
}

pub fn safe_truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// serde 辅助：把字符串/数字/布尔都转成 Option<String>。
pub fn de_opt_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.and_then(|v| match v {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }))
}

/// 必填字符串但接口可能返回 null：统一兜底为空字符串。
pub fn de_string_or_default<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(de_opt_string(d)?.unwrap_or_default())
}

/// 数组字段可能返回 null：统一兜底为空数组。
pub fn de_vec_or_default<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}
