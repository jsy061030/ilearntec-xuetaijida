use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use ilearn_core::api::ApiClient;
use ilearn_core::auth::{AuthClient, PasswordOutcome, Session, WxCodeStatus};
use ilearn_core::models::{CourseDump, Dump, Lesson, Record, RecordDump, Term, User};
use ilearn_core::util::{now_ms, open_file, prompt_password, prompt_required, prompt_select};

#[derive(Debug, Parser)]
#[command(
    name = "ilearn-cli",
    about = "学在吉大 CLI：登录并导出课程、录播和视频信息",
    version
)]
struct Cli {
    /// 使用企业微信扫码登录（默认使用密码 + 双验证码）
    #[arg(long)]
    qr: bool,

    /// 忽略本地会话缓存，强制重新登录
    #[arg(long)]
    fresh: bool,

    /// 学号/邮箱账号；不填则交互输入
    #[arg(long)]
    username: Option<String>,

    /// 缓存目录（会话、验证码、二维码、字幕）
    #[arg(long, default_value = "./.ilearn-cache")]
    cache_dir: PathBuf,

    /// 指定学年，例如 2025-2026
    #[arg(long)]
    term_year: Option<String>,

    /// 指定学期：1 或 2
    #[arg(long)]
    term: Option<String>,

    /// 指定课程 id（Lesson.id 或 courseId）
    #[arg(long)]
    course_id: Option<String>,

    /// 指定录播 resourceId（或 Record.id）
    #[arg(long)]
    resource_id: Option<String>,

    /// 导出所选学期的全部课程和录播（不交互选择）
    #[arg(long)]
    all: bool,

    /// 输出完整 JSON 到 stdout
    #[arg(long)]
    json: bool,

    /// 将完整 JSON 写入文件
    #[arg(long)]
    out: Option<PathBuf>,

    /// 只登录并保存会话，不查询课程
    #[arg(long)]
    login_only: bool,

    /// 同时下载字幕文件（保存到缓存目录 subtitles/）
    #[arg(long)]
    download_subtitle: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.cache_dir)
        .with_context(|| format!("创建缓存目录 {}", cli.cache_dir.display()))?;

    let session = ensure_session(&cli).await?;
    println!(
        "登录成功: {} / {}",
        session.user.username, session.user.realname
    );

    if cli.login_only {
        return Ok(());
    }

    let api = ApiClient::new(session.client.clone());
    let dump = build_dump(&api, &session.user, &cli).await?;
    output_dump(&dump, &cli)?;
    Ok(())
}

async fn ensure_session(cli: &Cli) -> Result<Session> {
    let session_path = cli.cache_dir.join("session.json");
    let mut auth = AuthClient::new(&cli.cache_dir)?;

    if !cli.fresh {
        match auth.load_session(&session_path) {
            Ok(true) => {
                if let Some(session) = auth.cached_session().await {
                    println!("使用本地缓存会话。");
                    return Ok(session);
                }
                log::info!("缓存会话已失效，重新登录");
                auth = AuthClient::new(&cli.cache_dir)?;
            }
            Ok(false) => {}
            Err(e) => {
                log::warn!("读取会话缓存失败，忽略: {e:#}");
                auth = AuthClient::new(&cli.cache_dir)?;
            }
        }
    }

    let session = if cli.qr {
        auth.qr_login().await?
    } else {
        password_login(&auth, cli).await?
    };

    if let Err(e) = auth.save_session(&session_path) {
        log::warn!("保存会话缓存失败: {e:#}");
    }

    Ok(session)
}

async fn password_login(auth: &AuthClient, cli: &Cli) -> Result<Session> {
    let username = match &cli.username {
        Some(u) => u.clone(),
        None => prompt_required("学号/邮箱账号: ")?,
    };
    let password = prompt_password("密码（输入不显示）: ")?;

    let form = auth.fetch_login_form().await?;
    match auth.submit_password(&form, &username, &password).await? {
        PasswordOutcome::Complete(session) => Ok(session),
        PasswordOutcome::NeedSecondFactor { lt, execution } => {
            let captcha_path = auth.fetch_captcha().await?;
            println!("图片验证码: {}", captcha_path.display());
            open_file(&captcha_path);

            let captcha = prompt_required("请输入图片验证码: ")?;
            match auth.request_wx_code(&captcha).await? {
                WxCodeStatus::Sent => {
                    println!("企业微信验证码已发送，请查看手机。");
                }
                WxCodeStatus::Unknown => {
                    println!("已请求企业微信验证码，但未读取到状态，请查看手机。");
                }
            }

            let wx_code = prompt_required("请输入企业微信验证码: ")?;
            auth.submit_verification(&lt, &execution, &captcha, &wx_code)
                .await
        }
    }
}

async fn build_dump(api: &ApiClient, user: &User, cli: &Cli) -> Result<Dump> {
    if let Err(e) = api.ensure_res_session().await {
        log::warn!("资源中心会话建立失败，视频信息可能不可用: {e:#}");
    }

    eprintln!("正在获取学期列表...");
    let terms = api.terms().await?;
    if terms.is_empty() {
        bail!("没有获取到学期，可能登录态或接口异常");
    }
    let term = select_term(&terms, cli)?;
    println!(
        "学期: {} 学年 第{}学期 {}",
        term.year, term.num, term.name
    );

    let weeks = match api.weeks(&term.id).await {
        Ok(w) => Some(w),
        Err(e) => {
            log::warn!("获取周列表失败（不影响主流程）: {e:#}");
            None
        }
    };

    eprintln!("正在获取课程列表...");
    let lessons = api.lessons(&term.year, &term.num).await?;
    if lessons.is_empty() {
        bail!("该学期没有课程");
    }
    let selected_lessons = select_lessons(lessons, cli)?;
    let total_lessons = selected_lessons.len();
    eprintln!("已选择 {} 门课程，开始获取录播和视频信息...", total_lessons);

    let mut courses = Vec::new();
    for (ci, lesson) in selected_lessons.into_iter().enumerate() {
        let course_name = lesson
            .course_name
            .clone()
            .or_else(|| lesson.name.clone())
            .unwrap_or_else(|| lesson.id.clone());
        eprintln!(
            "[{}/{}] 课程: {} ({})",
            ci + 1,
            total_lessons,
            course_name,
            lesson.id
        );

        let all_records = api.records(&term.id, &lesson.id).await?;
        let records = select_records(all_records, cli)?;
        if records.is_empty() && cli.resource_id.is_some() {
            // 指定了 resourceId，但这门课没有匹配项，继续找下一门。
            continue;
        }
        let total_records = records.len();

        let mut record_dumps = Vec::new();
        for (ri, record) in records.into_iter().enumerate() {
            let label = record
                .live_record_name
                .clone()
                .or_else(|| record.course_name.clone())
                .unwrap_or_else(|| record.resource_id.clone());
            eprintln!(
                "      录播 [{}/{}]: {} ({})",
                ri + 1,
                total_records,
                label,
                record.resource_id
            );

            let video = if record.resource_id.is_empty() {
                None
            } else {
                match api.video_resource(&record.resource_id).await {
                    Ok(v) => Some(v),
                    Err(e) => {
                        log::warn!("获取视频信息失败 {}: {e:#}", record.resource_id);
                        None
                    }
                }
            };
            let video_error = if record.resource_id.is_empty() {
                Some("该录播没有 resourceId（可能尚未生成视频）".to_string())
            } else if video.is_none() {
                Some(format!("resourceId {}", record.resource_id))
            } else {
                None
            };
            let subtitle_url = video.as_ref().and_then(|v| v.phase_url.clone());

            let (subtitle_file, subtitle_error) = if cli.download_subtitle {
                let bytes_result = if let Some(url) = &subtitle_url {
                    // 优先使用 videoClassInfo.phaseUrl 的签名地址；失败再走资源中心接口。
                    match api.download_bytes(url).await {
                        Ok(bytes) => Ok(bytes),
                        Err(e) => {
                            log::warn!("phaseUrl 下载失败，尝试资源中心字幕接口: {e:#}");
                            api.subtitle(&record.resource_id).await
                        }
                    }
                } else {
                    api.subtitle(&record.resource_id).await
                };

                match bytes_result {
                    Ok(bytes) => {
                        let dir = cli.cache_dir.join("subtitles");
                        if let Err(e) = std::fs::create_dir_all(&dir) {
                            (None, Some(format!("创建字幕目录失败: {e}")))
                        } else {
                            let path = dir.join(format!("{}.vtt", record.resource_id));
                            match std::fs::write(&path, bytes) {
                                Ok(()) => (Some(path.to_string_lossy().to_string()), None),
                                Err(e) => (None, Some(format!("写入字幕失败: {e}"))),
                            }
                        }
                    }
                    Err(e) => (None, Some(format!("{e:#}"))),
                }
            } else {
                (None, None)
            };

            record_dumps.push(RecordDump {
                record,
                video,
                video_error,
                subtitle_url,
                subtitle_file,
                subtitle_error,
            });
        }

        courses.push(CourseDump {
            lesson,
            records: record_dumps,
        });
    }

    if courses.is_empty() {
        bail!("没有匹配到任何课程/录播，请检查 --course-id / --resource-id");
    }

    Ok(Dump {
        generated_at_ms: now_ms(),
        user: user.clone(),
        term,
        weeks,
        courses,
    })
}

fn select_term(terms: &[Term], cli: &Cli) -> Result<Term> {
    if let (Some(year), Some(num)) = (&cli.term_year, &cli.term) {
        return terms
            .iter()
            .find(|t| &t.year == year && &t.num == num)
            .cloned()
            .ok_or_else(|| anyhow!("未找到学期 {year} 第{num}学期"));
    }

    if terms.len() == 1 {
        return Ok(terms[0].clone());
    }

    let labels: Vec<String> = terms
        .iter()
        .map(|t| format!("{} 学年 第{}学期 {} [{}]", t.year, t.num, t.name, t.id))
        .collect();
    let idx = prompt_select("请选择学期：", &labels)?;
    Ok(terms[idx].clone())
}

fn select_lessons(mut lessons: Vec<Lesson>, cli: &Cli) -> Result<Vec<Lesson>> {
    if let Some(cid) = &cli.course_id {
        let selected: Vec<Lesson> = lessons
            .into_iter()
            .filter(|l| {
                l.id == *cid || l.course_id.as_deref() == Some(cid.as_str())
            })
            .collect();
        if selected.is_empty() {
            bail!("未找到课程 id={cid}");
        }
        return Ok(selected);
    }

    // 指定 resourceId 但没有指定课程时，遍历所有课程才能找到对应录播。
    if cli.all || (cli.resource_id.is_some() && cli.course_id.is_none()) {
        return Ok(lessons);
    }

    if lessons.len() == 1 {
        return Ok(lessons);
    }

    let labels: Vec<String> = lessons
        .iter()
        .map(|l| {
            let name = l
                .course_name
                .as_deref()
                .or(l.name.as_deref())
                .unwrap_or(&l.id);
            let teacher = l.teacher_name.as_deref().unwrap_or("");
            format!("{} / {} [{}]", name, teacher, l.id)
        })
        .collect();
    let idx = prompt_select("请选择课程：", &labels)?;
    Ok(vec![lessons.remove(idx)])
}

fn select_records(mut records: Vec<Record>, cli: &Cli) -> Result<Vec<Record>> {
    if let Some(rid) = &cli.resource_id {
        let selected: Vec<Record> = records
            .into_iter()
            .filter(|r| r.resource_id == *rid || r.id == *rid)
            .collect();
        return Ok(selected);
    }

    if cli.all || records.len() <= 1 {
        return Ok(records);
    }

    let labels: Vec<String> = records
        .iter()
        .map(|r| {
            let name = r
                .live_record_name
                .as_deref()
                .or(r.course_name.as_deref())
                .unwrap_or(&r.resource_id);
            format!(
                "{} {} {} [{}]",
                r.current_date.as_deref().unwrap_or(""),
                name,
                r.time_range.as_deref().unwrap_or(""),
                r.resource_id
            )
        })
        .collect();
    let idx = prompt_select("请选择录播：", &labels)?;
    Ok(vec![records.remove(idx)])
}

fn output_dump(dump: &Dump, cli: &Cli) -> Result<()> {
    let json = serde_json::to_string_pretty(dump)?;

    if let Some(out) = &cli.out {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, &json)?;
        println!("完整 JSON 已写入: {}", out.display());
    }

    if cli.json {
        println!("{json}");
    } else {
        print_summary(dump);
    }

    Ok(())
}

fn print_summary(dump: &Dump) {
    println!("\n用户: {} / {}", dump.user.username, dump.user.realname);
    println!(
        "学期: {} 学年 第{}学期 {}",
        dump.term.year, dump.term.num, dump.term.name
    );
    if let Some(w) = &dump.weeks {
        println!(
            "当前周: {}，总周数: {}",
            w.current_week.as_deref().unwrap_or("?"),
            w.week_list.len()
        );
    }
    println!("课程数: {}", dump.courses.len());

    for (ci, course) in dump.courses.iter().enumerate() {
        let name = course
            .lesson
            .course_name
            .as_deref()
            .or(course.lesson.name.as_deref())
            .unwrap_or(&course.lesson.id);
        println!(
            "\n[{}] {} / {}  (id={})",
            ci + 1,
            name,
            course.lesson.teacher_name.as_deref().unwrap_or(""),
            course.lesson.id
        );
        println!("    录播数: {}", course.records.len());

        for (ri, rd) in course.records.iter().enumerate() {
            let name = rd
                .record
                .live_record_name
                .as_deref()
                .or(rd.record.course_name.as_deref())
                .unwrap_or(&rd.record.resource_id);
            println!(
                "    {}. {} {} {}",
                ri + 1,
                rd.record.current_date.as_deref().unwrap_or(""),
                name,
                rd.record.time_range.as_deref().unwrap_or("")
            );
            println!("       resourceId: {}", rd.record.resource_id);

            if let Some(video) = &rd.video {
                if let Some(resource_name) = &video.resource_name {
                    println!("       资源: {resource_name}");
                }
                for v in &video.video_list {
                    println!(
                        "       - {}  大小: {}",
                        v.video_name.as_deref().unwrap_or("视频"),
                        human_size(v.video_size.as_deref().unwrap_or(""))
                    );
                    if let Some(url) = &v.video_path {
                        println!("         {url}");
                    }
                }
            } else if let Some(err) = &rd.video_error {
                println!("       视频信息获取失败: {err}");
            }

            if let Some(url) = &rd.subtitle_url {
                println!("       字幕 URL: {url}");
            }
            if let Some(path) = &rd.subtitle_file {
                println!("       字幕已下载: {path}");
            }
            if let Some(err) = &rd.subtitle_error {
                println!("       字幕下载失败: {err}");
            }
        }
    }
}

fn human_size(s: &str) -> String {
    match s.parse::<u64>() {
        Ok(n) if n >= 1_073_741_824 => format!("{:.2} GB", n as f64 / 1_073_741_824.0),
        Ok(n) if n >= 1_048_576 => format!("{:.1} MB", n as f64 / 1_048_576.0),
        Ok(n) => format!("{n} B"),
        Err(_) => s.to_string(),
    }
}
