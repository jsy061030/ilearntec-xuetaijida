<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    api,
    errorText,
    humanSize,
    type Lesson,
    type PasswordStart,
    type RecordItem,
    type Term,
    type User,
    type VideoResource,
  } from "$lib/api";

  let booting = $state(true);
  let proxyBase = $state("");
  let user = $state<User | null>(null);
  let page = $state<"login" | "browse">("login");
  let error = $state("");
  let notice = $state("");

  let loginMode = $state<"qr" | "password">("qr");
  let qrImage = $state("");
  let qrStatus = $state("正在准备二维码...");
  let qrTimer: number | undefined;
  let qrGeneration = 0;

  let username = $state("");
  let password = $state("");
  let passwordBusy = $state(false);
  let loginStep = $state<"credentials" | "captcha" | "wx">("credentials");
  let captchaImage = $state("");
  let captcha = $state("");
  let wxCode = $state("");
  let wxStatus = $state("");

  let terms = $state<Term[]>([]);
  let selectedTerm = $state<Term | null>(null);
  let lessons = $state<Lesson[]>([]);
  let selectedLesson = $state<Lesson | null>(null);
  let records = $state<RecordItem[]>([]);
  let selectedRecord = $state<RecordItem | null>(null);

  let loadingTerms = $state(false);
  let loadingLessons = $state(false);
  let loadingRecords = $state(false);
  let courseQuery = $state("");
  let recordQuery = $state("");
  let sidebarCollapsed = $state(false);

  let video = $state<VideoResource | null>(null);
  let videoBusy = $state(false);
  let videoRequest = 0;
  let browseError = $state("");
  let playerError = $state("");
  let subtitleError = $state("");
  let teacherBuffering = $state(false);
  let hdmiBuffering = $state(false);
  let lastHdmiSync = 0;
  let cues = $state<{ start: number; end: number; text: string }[]>([]);
  let activeCue = $state(-1);
  let autoScroll = $state(true);

  let teacherEl = $state.raw<HTMLVideoElement | undefined>(undefined);
  let hdmiEl = $state.raw<HTMLVideoElement | undefined>(undefined);
  let subtitleEl = $state.raw<HTMLDivElement | undefined>(undefined);

  let teacher = $derived(video?.videoList?.[0] ?? null);
  let hdmi = $derived(video?.videoList?.[1] ?? null);
  let courseName = $derived(
    selectedLesson?.courseName ?? selectedLesson?.name ?? "课程",
  );
  let userInitial = $derived(
    (user?.realname ?? user?.username ?? "?").trim().slice(0, 1).toUpperCase(),
  );
  let filteredLessons = $derived.by(() => {
    const q = courseQuery.trim().toLowerCase();
    if (!q) return lessons;
    return lessons.filter((l) =>
      `${l.courseName ?? l.name ?? ""} ${l.teacherName ?? ""}`
        .toLowerCase()
        .includes(q),
    );
  });
  let filteredRecords = $derived.by(() => {
    const q = recordQuery.trim().toLowerCase();
    if (!q) return records;
    return records.filter((r) =>
      `${r.liveRecordName ?? ""} ${r.currentDate ?? ""} ${r.timeRange ?? ""} ${r.roomName ?? ""}`
        .toLowerCase()
        .includes(q),
    );
  });

  onMount(() => {
    (async () => {
      try {
        proxyBase = await api.getProxyBase();
      } catch {
        proxyBase = "";
      }
      try {
        const cached = await api.loadSession();
        if (cached) {
          await enterBrowse(cached);
        } else {
          await startQr();
        }
      } catch (e) {
        error = errorText(e);
      } finally {
        booting = false;
      }
    })();
  });

  onDestroy(() => {
    stopQr();
  });

  function mediaSrc(url?: string | null): string {
    if (!url) return "";
    if (!proxyBase) return url;
    return `${proxyBase}/media?url=${encodeURIComponent(url)}`;
  }

  function flash(text: string) {
    notice = text;
    window.setTimeout(() => {
      if (notice === text) notice = "";
    }, 1800);
  }

  function initial(name?: string | null): string {
    return (name ?? "?").trim().slice(0, 1).toUpperCase() || "?";
  }

  function dateParts(date?: string | null): { day: string; month: string } {
    if (!date) return { day: "--", month: "暂无" };
    const parts = date.split("-");
    if (parts.length < 3) return { day: date, month: "" };
    return {
      day: parts[2],
      month: `${Number(parts[1])}月`,
    };
  }

  function recordName(r: RecordItem | null): string {
    return r?.liveRecordName ?? r?.courseName ?? "录播";
  }

  async function enterBrowse(u: User) {
    user = u;
    page = "browse";
    error = "";
    notice = "";
    stopQr();
    await loadTerms();
  }

  async function loadTerms() {
    loadingTerms = true;
    browseError = "";
    terms = [];
    lessons = [];
    records = [];
    selectedTerm = null;
    selectedLesson = null;
    selectedRecord = null;
    courseQuery = "";
    recordQuery = "";
    clearVideo();
    try {
      terms = await api.getTerms();
      if (terms.length > 0) {
        await chooseTerm(terms[0]);
      } else {
        browseError = "没有获取到学期";
      }
    } catch (e) {
      browseError = errorText(e);
    } finally {
      loadingTerms = false;
    }
  }

  async function chooseTerm(term: Term) {
    selectedTerm = term;
    selectedLesson = null;
    lessons = [];
    records = [];
    selectedRecord = null;
    courseQuery = "";
    recordQuery = "";
    clearVideo();
    browseError = "";
    loadingLessons = true;
    try {
      lessons = await api.getLessons(term.year, term.num);
      if (lessons.length > 0) await chooseLesson(lessons[0]);
    } catch (e) {
      browseError = errorText(e);
    } finally {
      loadingLessons = false;
    }
  }

  async function chooseLesson(lesson: Lesson) {
    selectedLesson = lesson;
    records = [];
    selectedRecord = null;
    recordQuery = "";
    clearVideo();
    browseError = "";
    if (!selectedTerm) return;
    loadingRecords = true;
    try {
      records = await api.getRecords(selectedTerm.id, lesson.id);
    } catch (e) {
      browseError = errorText(e);
    } finally {
      loadingRecords = false;
    }
  }

  function onTermChange(event: Event) {
    const id = (event.currentTarget as HTMLSelectElement).value;
    const term = terms.find((t) => t.id === id);
    if (term) void chooseTerm(term);
  }

  async function chooseRecord(record: RecordItem) {
    selectedRecord = record;
    clearVideo();
    const req = ++videoRequest;
    playerError = "";
    browseError = "";
    if (!record.resourceId) {
      browseError = "这条录播还没有 resourceId，可能视频尚未生成";
      return;
    }
    videoBusy = true;
    try {
      const v = await api.getVideo(record.resourceId);
      if (req !== videoRequest) return;
      video = v;
      if (v.phaseUrl) {
        try {
          const text = await api.getSubtitle(v.phaseUrl);
          if (req !== videoRequest) return;
          setSubtitle(text);
        } catch (e) {
          if (req === videoRequest) subtitleError = errorText(e);
        }
      }
    } catch (e) {
      if (req === videoRequest) browseError = errorText(e);
    } finally {
      if (req === videoRequest) videoBusy = false;
    }
  }

  function parseTimestamp(raw: string): number | null {
    const m = raw
      .trim()
      .match(/^(?:(\d+):)?(\d{1,2}):(\d{2})(?:[.,](\d{1,3}))?$/);
    if (!m) return null;
    const h = Number(m[1] ?? 0);
    const min = Number(m[2]);
    const sec = Number(m[3]);
    const ms = Number((m[4] ?? "0").padEnd(3, "0"));
    return h * 3600 + min * 60 + sec + ms / 1000;
  }

  function parseVtt(text: string): { start: number; end: number; text: string }[] {
    const normalized = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
    const result: { start: number; end: number; text: string }[] = [];
    for (const block of normalized.split(/\n{2,}/)) {
      const lines = block
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
      const timingIndex = lines.findIndex((line) => line.includes("-->"));
      if (timingIndex < 0) continue;
      const [from, to] = lines[timingIndex].split("-->");
      const start = parseTimestamp(from);
      const end = parseTimestamp((to ?? "").trim().split(/\s+/)[0] ?? "");
      const content = lines
        .slice(timingIndex + 1)
        .join(" ")
        .replace(/<[^>]+>/g, "")
        .trim();
      if (start !== null && end !== null && content) {
        result.push({ start, end, text: content });
      }
    }
    return result;
  }

  function formatTime(seconds: number): string {
    const total = Math.max(0, Math.floor(seconds));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    return h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
      : `${m}:${String(s).padStart(2, "0")}`;
  }

  function setSubtitle(text: string) {
    cues = parseVtt(text);
    activeCue = -1;
    subtitleError = cues.length === 0 ? "字幕格式无法解析" : "";
    if (subtitleEl) subtitleEl.scrollTop = 0;
  }

  function scrollToCue(index: number) {
    if (!subtitleEl) return;
    const el = subtitleEl.querySelector<HTMLElement>(`[data-cue="${index}"]`);
    if (!el) return;
    subtitleEl.scrollTo({
      top: el.offsetTop - subtitleEl.clientHeight / 2 + el.clientHeight / 2,
      behavior: "smooth",
    });
  }

  function updateCue(time: number) {
    if (cues.length === 0) return;
    const index = cues.findIndex((cue) => time >= cue.start && time < cue.end);
    if (index >= 0 && index !== activeCue) {
      activeCue = index;
      if (autoScroll) scrollToCue(index);
    }
  }

  function seekToCue(seconds: number) {
    for (const el of [teacherEl, hdmiEl]) {
      if (!el) continue;
      try {
        el.currentTime = seconds;
      } catch {
        /* metadata not ready */
      }
    }
    updateCue(seconds);
  }

  function clearVideo() {
    videoRequest += 1;
    video = null;
    videoBusy = false;
    playerError = "";
    subtitleError = "";
    teacherBuffering = false;
    hdmiBuffering = false;
    cues = [];
    activeCue = -1;
  }

  function stopQr() {
    qrGeneration += 1;
    if (qrTimer !== undefined) {
      window.clearTimeout(qrTimer);
      qrTimer = undefined;
    }
  }

  async function startQr() {
    stopQr();
    qrGeneration += 1;
    const gen = qrGeneration;
    qrImage = "";
    qrStatus = "正在生成二维码...";
    error = "";
    try {
      const r = await api.startQr();
      if (gen !== qrGeneration) return;
      qrImage = r.image;
      qrStatus = "请使用企业微信扫码";
      scheduleQr(gen);
    } catch (e) {
      error = errorText(e);
      qrStatus = "二维码生成失败";
    }
  }

  function scheduleQr(gen: number) {
    qrTimer = window.setTimeout(async () => {
      if (gen !== qrGeneration) return;
      try {
        const r = await api.pollQr();
        if (gen !== qrGeneration) return;
        if (r.status === "scanned" && r.user) {
          qrStatus = "扫码成功";
          await enterBrowse(r.user);
          return;
        }
        if (r.status === "expired") {
          qrStatus = "二维码已失效，请重新生成";
          return;
        }
        qrStatus = "等待扫码...";
        scheduleQr(gen);
      } catch {
        qrStatus = "轮询失败，稍后重试...";
        scheduleQr(gen);
      }
    }, 3000);
  }

  function switchMode(mode: "qr" | "password") {
    loginMode = mode;
    error = "";
    if (mode === "qr") {
      void startQr();
    } else {
      stopQr();
    }
  }

  async function submitCredentials(event: Event) {
    event.preventDefault();
    error = "";
    if (!username.trim() || !password) {
      error = "请输入账号和密码";
      return;
    }
    passwordBusy = true;
    loginStep = "credentials";
    try {
      const r: PasswordStart = await api.startPassword(username.trim(), password);
      if (r.needSecondFactor) {
        captchaImage = r.captcha ?? "";
        captcha = "";
        wxCode = "";
        wxStatus = "";
        loginStep = "captcha";
      } else if (r.user) {
        await enterBrowse(r.user);
      }
    } catch (e) {
      error = errorText(e);
    } finally {
      passwordBusy = false;
    }
  }

  async function refreshCaptcha() {
    error = "";
    try {
      captchaImage = await api.refreshCaptcha();
      captcha = "";
    } catch (e) {
      error = errorText(e);
    }
  }

  async function requestWx() {
    error = "";
    if (!captcha.trim()) {
      error = "请先输入图片验证码";
      return;
    }
    passwordBusy = true;
    wxStatus = "正在请求企业微信验证码...";
    try {
      const status = await api.requestWx(captcha.trim());
      wxStatus =
        status === "sent"
          ? "验证码已发送，请查看企业微信"
          : "已请求验证码，请查看企业微信";
      loginStep = "wx";
    } catch (e) {
      error = errorText(e);
      wxStatus = "";
    } finally {
      passwordBusy = false;
    }
  }

  async function submitWx() {
    error = "";
    if (!wxCode.trim()) {
      error = "请输入企业微信验证码";
      return;
    }
    passwordBusy = true;
    try {
      const u = await api.submitWx(captcha.trim(), wxCode.trim());
      await enterBrowse(u);
    } catch (e) {
      error = errorText(e);
    } finally {
      passwordBusy = false;
    }
  }

  async function logout() {
    stopQr();
    try {
      await api.logout();
    } catch {
      /* ignore */
    }
    user = null;
    page = "login";
    terms = [];
    lessons = [];
    records = [];
    selectedTerm = null;
    selectedLesson = null;
    selectedRecord = null;
    clearVideo();
    error = "";
    notice = "";
    loginStep = "credentials";
    loginMode = "qr";
    await startQr();
  }

  async function copy(text?: string | null) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      flash("已复制到剪贴板");
      return;
    } catch {
      /* fall back */
    }
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.left = "-9999px";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
      flash("已复制到剪贴板");
    } catch (e) {
      error = `复制失败：${errorText(e)}`;
    }
  }

  function playBoth() {
    if (teacherEl) {
      teacherEl.play().catch(() => undefined);
      if (hdmiEl) {
        hdmiEl.play().catch(() => undefined);
      }
    } else if (hdmiEl) {
      hdmiEl.play().catch(() => undefined);
    }
  }

  function pauseBoth() {
    teacherEl?.pause();
    hdmiEl?.pause();
  }

  /**
   * 只在显式点击“HDMI 对齐”或播放开始时对齐一次。
   * 之前每次 teacher timeupdate 都改 hdmi.currentTime，会触发频繁 seek，
   * 表现就是 HDMI 短暂停住后恢复。
   */
  function syncHdmi(force = false) {
    if (!teacherEl || !hdmiEl) return;
    const drift = Math.abs(hdmiEl.currentTime - teacherEl.currentTime);
    const now = Date.now();
    if (!force && (drift < 1.2 || now - lastHdmiSync < 5000)) return;
    try {
      hdmiEl.currentTime = teacherEl.currentTime;
      lastHdmiSync = now;
    } catch {
      /* metadata not ready */
    }
  }

  function markTeacherBuffering(value: boolean) {
    teacherBuffering = value;
  }

  function markHdmiBuffering(value: boolean) {
    hdmiBuffering = value;
  }
</script>

<svelte:head>
  <title>学在吉大</title>
</svelte:head>

{#if booting}
  <div class="boot-screen">
    <div class="boot-mark">学</div>
    <div class="boot-name">学在吉大</div>
    <div class="boot-hint">正在连接统一身份认证...</div>
    <div class="boot-track"><span></span></div>
  </div>
{:else if page === "login"}
  <div class="login-screen">
    <div class="login-orb orb-a"></div>
    <div class="login-orb orb-b"></div>
    <div class="login-grid"></div>

    <div class="login-card">
      <aside class="login-brand">
        <div class="brand-lockup">
          <div class="brand-mark">学</div>
          <div>
            <div class="brand-title">学在吉大</div>
            <div class="brand-caption">JLU · 课程录播</div>
          </div>
        </div>

        <div class="brand-hero">
          <h1>把课堂<br />装进桌面。</h1>
          <p>统一认证登录，浏览学期课程，双路回看教师机位与 HDMI 投影。</p>
        </div>

        <div class="brand-features">
          <div class="feature">
            <span>01</span>
            <div><b>企业微信 / 密码登录</b><small>支持扫码与双验证码</small></div>
          </div>
          <div class="feature">
            <span>02</span>
            <div><b>教师机位 + HDMI</b><small>双路视频同屏播放</small></div>
          </div>
          <div class="feature">
            <span>03</span>
            <div><b>VTT 字幕</b><small>自动加载、随视频显示</small></div>
          </div>
        </div>

        <div class="brand-foot">非官方客户端 · 仅限个人学习使用</div>
      </aside>

      <section class="login-panel">
        <header class="login-head">
          <div>
            <h2>欢迎回来</h2>
            <p>使用企业微信扫码，或统一身份认证登录</p>
          </div>
          <span class="secure-pill">CAS 安全登录</span>
        </header>

        <div class="segmented">
          <button
            class:active={loginMode === "qr"}
            onclick={() => switchMode("qr")}>扫码登录</button
          >
          <button
            class:active={loginMode === "password"}
            onclick={() => switchMode("password")}>密码登录</button
          >
        </div>

        {#if error}
          <div class="alert danger">{error}</div>
        {/if}
        {#if notice}
          <div class="alert success">{notice}</div>
        {/if}

        {#if loginMode === "qr"}
          <div class="qr-panel">
            <div class="qr-frame" class:ready={!!qrImage}>
              {#if qrImage}
                <img src={qrImage} alt="企业微信登录二维码" />
                <div class="qr-corners" aria-hidden="true"></div>
              {:else}
                <div class="qr-loading">
                  <div class="spinner"></div>
                  <span>二维码生成中</span>
                </div>
              {/if}
            </div>
            <div class="qr-title">企业微信扫码登录</div>
            <div class="qr-status">{qrStatus}</div>
            <button class="btn soft" onclick={startQr}>刷新二维码</button>
            <div class="qr-steps">
              <span>打开企业微信</span><i></i><span>扫一扫</span><i></i
              ><span>确认登录</span>
            </div>
          </div>
        {:else}
          <div class="step-strip">
            <div class:active={loginStep === "credentials"} class:done={loginStep !== "credentials"}>
              <b>1</b><span>账号密码</span>
            </div>
            <i></i>
            <div class:active={loginStep === "captcha"} class:done={loginStep === "wx"}>
              <b>2</b><span>安全验证</span>
            </div>
            <i></i>
            <div class:active={loginStep === "wx"}>
              <b>3</b><span>企业微信</span>
            </div>
          </div>

          {#if loginStep === "credentials"}
            <form class="auth-form" onsubmit={submitCredentials}>
              <label>
                <span>学号 / 邮箱账号</span>
                <input bind:value={username} autocomplete="username" placeholder="例如 jlu2424..." />
              </label>
              <label>
                <span>统一身份认证密码</span>
                <input
                  bind:value={password}
                  type="password"
                  autocomplete="current-password"
                  placeholder="输入密码"
                />
              </label>
              <button class="btn primary wide" type="submit" disabled={passwordBusy}>
                {passwordBusy ? "正在提交..." : "下一步"}
              </button>
              <p class="form-hint">密码提交后还需要图片验证码与企业微信验证码。</p>
            </form>
          {:else if loginStep === "captcha"}
            <div class="auth-form">
              <div class="captcha-card">
                {#if captchaImage}
                  <img src={captchaImage} alt="图片验证码" />
                {:else}
                  <span class="muted">验证码加载失败</span>
                {/if}
                <button class="text-btn" onclick={refreshCaptcha}>换一张</button>
              </div>
              <label>
                <span>图片验证码</span>
                <input bind:value={captcha} maxlength="8" placeholder="输入图片中的字符" />
              </label>
              <button class="btn primary wide" onclick={requestWx} disabled={passwordBusy}>
                {passwordBusy ? "正在请求..." : "获取企业微信验证码"}
              </button>
              <button class="btn ghost wide" onclick={() => (loginStep = "credentials")}>
                返回上一步
              </button>
            </div>
          {:else}
            <div class="auth-form">
              <div class="wx-banner">
                <div class="wx-dot"></div>
                <div>
                  <b>验证码已发送</b>
                  <span>{wxStatus || "请查看企业微信，输入收到的验证码"}</span>
                </div>
              </div>
              <label>
                <span>企业微信验证码</span>
                <input bind:value={wxCode} maxlength="8" placeholder="输入 6 位数字" />
              </label>
              <button class="btn primary wide" onclick={submitWx} disabled={passwordBusy}>
                {passwordBusy ? "正在验证..." : "确认登录"}
              </button>
              <button class="btn ghost wide" onclick={() => (loginStep = "captcha")}>
                返回验证码页
              </button>
            </div>
          {/if}
        {/if}

        <footer class="login-foot">
          登录即表示同意仅将本工具用于个人课程学习，不用于传播课程资源。
        </footer>
      </section>
    </div>
  </div>
{:else}
  <div class="app-shell">
    <header class="appbar">
      <div class="appbar-brand">
        <div class="appbar-mark">学</div>
        <div>
          <b>学在吉大</b>
          <span>课程录播</span>
        </div>
      </div>

      <button
        class="icon-btn rail-btn"
        class:active={!sidebarCollapsed}
        title={sidebarCollapsed ? "展开课程栏" : "收起课程栏"}
        aria-label={sidebarCollapsed ? "展开课程栏" : "收起课程栏"}
        onclick={() => (sidebarCollapsed = !sidebarCollapsed)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect x="3" y="4" width="18" height="16" rx="2"></rect>
          <path d="M9 4v16"></path>
        </svg>
      </button>

      <div class="appbar-divider"></div>

      <label class="term-picker">
        <span>学期</span>
        <select value={selectedTerm?.id ?? ""} onchange={onTermChange} disabled={loadingTerms}>
          {#each terms as term (term.id)}
            <option value={term.id}>
              {term.year} 学年 · 第{term.num}学期 · {term.name}
            </option>
          {/each}
        </select>
      </label>

      <div class="appbar-spacer"></div>

      <div class="connection">
        <i></i>
        <span>已连接</span>
      </div>

      <div class="user-chip">
        <div class="avatar">{userInitial}</div>
        <div class="user-text">
          <b>{user?.realname ?? user?.username ?? ""}</b>
          <span>{user?.username ?? ""}</span>
        </div>
      </div>

      <button class="icon-btn" title="退出登录" onclick={logout}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M9 4H5a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h4M16 17l5-5-5-5M21 12H9"
          />
        </svg>
      </button>
    </header>

    {#if error}
      <div class="alert danger page-alert">{error}</div>
    {/if}
    {#if notice}
      <div class="alert success page-alert">{notice}</div>
    {/if}
    {#if browseError}
      <div class="alert danger page-alert">{browseError}</div>
    {/if}

    <div class="workspace" class:collapsed={sidebarCollapsed}>
      <aside class="course-rail" class:collapsed={sidebarCollapsed}>
        <div class="rail-head">
          <div class="rail-copy">
            <span class="eyebrow">COURSES</span>
            <h2>课程</h2>
            <p>{selectedTerm ? `${selectedTerm.year} 学年 · 第${selectedTerm.num}学期` : "选择学期"}</p>
          </div>
          <span class="count-pill">{lessons.length}</span>
        </div>

        <label class="search-box">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="11" cy="11" r="7"></circle>
            <path d="m20 20-4-4"></path>
          </svg>
          <input bind:value={courseQuery} placeholder="搜索课程或教师" />
        </label>

        <div class="course-list">
          {#if loadingTerms || loadingLessons}
            {#each Array(5) as _}
              <div class="course-skeleton">
                <span></span>
                <div><i></i><i></i></div>
              </div>
            {/each}
          {:else if filteredLessons.length === 0}
            <div class="list-empty">没有匹配的课程</div>
          {:else}
            {#each filteredLessons as lesson (lesson.id)}
              <button
                class="course-item"
                class:active={selectedLesson?.id === lesson.id}
                title={lesson.courseName ?? lesson.name ?? lesson.id}
                onclick={() => chooseLesson(lesson)}
              >
                <div class="course-avatar">{initial(lesson.courseName ?? lesson.name)}</div>
                <div class="course-copy">
                  <b>{lesson.courseName ?? lesson.name ?? lesson.id}</b>
                  <span>{lesson.teacherName ?? "未标注教师"}</span>
                </div>
                <span class="chev">›</span>
              </button>
            {/each}
          {/if}
        </div>

        <div class="rail-foot">
          <span>当前学期</span>
          <b>{selectedTerm ? `${selectedTerm.year} · ${selectedTerm.num}` : "-"}</b>
        </div>
      </aside>

      <section class="record-panel">
        <div class="panel-head">
          <div>
            <span class="eyebrow">RECORDINGS</span>
            <h2>{courseName}</h2>
            <p>{selectedLesson?.teacherName ?? "选择一门课程"} · {records.length} 条录播</p>
          </div>
        </div>

        <label class="search-box">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="11" cy="11" r="7"></circle>
            <path d="m20 20-4-4"></path>
          </svg>
          <input bind:value={recordQuery} placeholder="搜索日期、教室" />
        </label>

        <div class="record-list">
          {#if loadingRecords}
            {#each Array(5) as _}
              <div class="record-skeleton">
                <span></span>
                <div><i></i><i></i><i></i></div>
              </div>
            {/each}
          {:else if filteredRecords.length === 0}
            <div class="list-empty">这门课暂无录播</div>
          {:else}
            {#each filteredRecords as record (record.id || record.resourceId)}
              {@const parts = dateParts(record.currentDate)}
              <button
                class="record-item"
                class:active={selectedRecord?.id === record.id}
                onclick={() => chooseRecord(record)}
                disabled={!record.resourceId}
              >
                <div class="date-tile">
                  <b>{parts.day}</b>
                  <span>{parts.month}</span>
                </div>
                <div class="record-copy">
                  <b>{recordName(record)}</b>
                  <span>
                    {record.timeRange ?? "时间未标注"}
                    {#if record.section} · 第{record.section}节{/if}
                  </span>
                  <span>{record.buildingName ?? ""} {record.roomName ?? ""}</span>
                </div>
                <div class="record-badge" class:muted={!record.resourceId}>
                  {#if record.resourceId}▶{:else}待生成{/if}
                </div>
              </button>
            {/each}
          {/if}
        </div>
      </section>

      <main class="stage">
        <div class="stage-head">
          <div>
            <span class="stage-eyebrow">NOW PLAYING</span>
            <h2>{video?.resourceName ?? (selectedRecord ? recordName(selectedRecord) : "选择一条录播")}</h2>
            <p>
              {selectedRecord?.currentDate ?? "选择一条录播"}
              {#if selectedRecord?.timeRange} · {selectedRecord.timeRange}{/if}
              {#if selectedRecord?.teacherName} · {selectedRecord.teacherName}{/if}
            </p>
          </div>

          {#if video}
            <div class="stage-tools">
              <button class="stage-btn" onclick={playBoth}>双路播放</button>
              <button class="stage-btn" onclick={pauseBoth}>暂停</button>
              <button class="stage-btn" onclick={() => syncHdmi(true)}>HDMI 对齐</button>
            </div>
          {/if}
        </div>

        {#if videoBusy}
          <div class="stage-loading">
            <div class="spinner"></div>
            <b>正在获取视频信息</b>
            <span>解析教师机位、HDMI 和字幕地址...</span>
          </div>
        {:else if video}
          <div class:two-videos={!!hdmi} class="video-grid">
            {#if teacher}
              <div class="video-card">
                <div class="video-head">
                  <span>{teacher.videoName ?? "教师机位"}</span>
                  <small>{humanSize(teacher.videoSize)}</small>
                  <button class="text-btn light" onclick={() => copy(teacher.videoPath)}>复制地址</button>
                </div>
                {#if teacher.videoPath}
                  <div class="video-body">
                    <!-- svelte-ignore a11y_media_has_caption -->
                    <video
                      bind:this={teacherEl}
                      src={mediaSrc(teacher.videoPath)}
                      controls
                      preload="auto"
                      playsinline
                      ontimeupdate={() => updateCue(teacherEl?.currentTime ?? 0)}
                      onwaiting={() => markTeacherBuffering(true)}
                      onplaying={() => markTeacherBuffering(false)}
                      oncanplay={() => markTeacherBuffering(false)}
                      onerror={() => {
                        markTeacherBuffering(false);
                        playerError = "教师机位视频加载失败，签名地址可能已过期";
                      }}
                    ></video>
                    {#if teacherBuffering}
                      <div class="video-buffer"><span class="spinner"></span>缓冲中</div>
                    {/if}
                  </div>
                {:else}
                  <div class="video-missing">没有教师机位地址</div>
                {/if}
              </div>
            {/if}

            {#if hdmi}
              <div class="video-card">
                <div class="video-head">
                  <span>{hdmi.videoName ?? "HDMI"}</span>
                  <small>{humanSize(hdmi.videoSize)}</small>
                  <button class="text-btn light" onclick={() => copy(hdmi.videoPath)}>复制地址</button>
                </div>
                {#if hdmi.videoPath}
                  <div class="video-body">
                    <!-- svelte-ignore a11y_media_has_caption -->
                    <video
                      bind:this={hdmiEl}
                      src={mediaSrc(hdmi.videoPath)}
                      controls
                      preload="auto"
                      playsinline
                      ontimeupdate={() => updateCue(hdmiEl?.currentTime ?? 0)}
                      onwaiting={() => markHdmiBuffering(true)}
                      onplaying={() => markHdmiBuffering(false)}
                      oncanplay={() => markHdmiBuffering(false)}
                      onerror={() => {
                        markHdmiBuffering(false);
                        playerError = "HDMI 视频加载失败，签名地址可能已过期";
                      }}
                    ></video>
                    {#if hdmiBuffering}
                      <div class="video-buffer"><span class="spinner"></span>缓冲中</div>
                    {/if}
                  </div>
                {:else}
                  <div class="video-missing">没有 HDMI 地址</div>
                {/if}
              </div>
            {/if}
          </div>

          <section class="subtitle-panel">
            <div class="subtitle-head">
              <div>
                <span class="stage-eyebrow">SUBTITLES</span>
                <b>字幕</b>
                <span class="subtitle-count">{cues.length} 条</span>
              </div>
              <div class="subtitle-actions">
                <button
                  class="text-btn light"
                  onclick={() => (autoScroll = !autoScroll)}
                >
                  {autoScroll ? "跟随播放" : "已暂停跟随"}
                </button>
                {#if video?.phaseUrl}
                  <button class="text-btn light" onclick={() => copy(video?.phaseUrl)}>复制地址</button>
                {/if}
              </div>
            </div>
            <div class="subtitle-scroll" bind:this={subtitleEl}>
              {#if cues.length > 0}
                {#each cues as cue, index (index)}
                  <button
                    class="cue-line"
                    class:active={index === activeCue}
                    data-cue={index}
                    title={`跳转到 ${formatTime(cue.start)}`}
                    onclick={() => seekToCue(cue.start)}
                  >
                    <span class="cue-time">{formatTime(cue.start)}</span>
                    <span class="cue-text">{cue.text}</span>
                  </button>
                {/each}
              {:else}
                <div class="subtitle-empty">
                  {subtitleError || "无字幕，或字幕尚未加载"}
                </div>
              {/if}
            </div>
          </section>

          {#if playerError}
            <div class="alert danger stage-alert">{playerError}</div>
          {/if}

          <div class="stage-info">
            <div>
              <span>录播日期</span>
              <b>{selectedRecord?.currentDate ?? "—"}</b>
            </div>
            <div>
              <span>时间 / 节次</span>
              <b>
                {selectedRecord?.timeRange ?? "—"}
                {#if selectedRecord?.section} · 第{selectedRecord.section}节{/if}
              </b>
            </div>
            <div>
              <span>教室</span>
              <b>{selectedRecord?.buildingName ?? ""} {selectedRecord?.roomName ?? "—"}</b>
            </div>
            <div>
              <span>资源 ID</span>
              <b class="mono" title={selectedRecord?.resourceId ?? ""}>
                {selectedRecord?.resourceId ?? "—"}
              </b>
            </div>
          </div>

          <div class="stage-footer">
            <div class="subtitle-state">
              {#if cues.length > 0}
                <span class="dot success"></span><span>字幕已加载 · {cues.length} 条</span>
              {:else if subtitleError}
                <span class="dot danger"></span><span>字幕加载失败</span>
              {:else}
                <span class="dot muted"></span><span>无字幕</span>
              {/if}
            </div>
            <div class="stage-actions">
              {#if video?.phaseUrl}
                <button class="text-btn light" onclick={() => copy(video?.phaseUrl)}>复制字幕地址</button>
              {/if}
              <button class="text-btn light" onclick={() => copy(selectedRecord?.resourceId)}>
                复制 resourceId
              </button>
            </div>
          </div>
        {:else}
          <div class="stage-empty">
            <div class="play-orb">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 6 10 6-10 6V6Z" /></svg>
            </div>
            <b>选择一条录播开始播放</b>
            <span>视频和字幕都是 24 小时有效的签名地址；过期后重新选择即可。</span>
            <div class="stage-tips">
              <span>教师机位</span><i></i><span>HDMI</span><i></i><span>VTT 字幕</span>
            </div>
          </div>
        {/if}
      </main>
    </div>
  </div>
{/if}

<style>
  .boot-screen {
    min-height: 100vh;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 10px;
    background:
      radial-gradient(circle at 50% 20%, rgba(79, 70, 229, 0.22), transparent 45%),
      #070b14;
    color: #e2e8f0;
  }
  .boot-mark {
    width: 64px;
    height: 64px;
    border-radius: 20px;
    display: grid;
    place-items: center;
    font-size: 28px;
    font-weight: 800;
    color: #fff;
    background: linear-gradient(145deg, #6366f1, #0d9488);
    box-shadow: 0 18px 50px rgba(79, 70, 229, 0.4);
  }
  .boot-name {
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }
  .boot-hint {
    color: #94a3b8;
    font-size: 12px;
  }
  .boot-track {
    width: 190px;
    height: 3px;
    border-radius: 99px;
    background: #1e293b;
    overflow: hidden;
    margin-top: 8px;
  }
  .boot-track span {
    display: block;
    width: 45%;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #6366f1, #14b8a6);
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(260%); }
  }

  .login-screen {
    position: relative;
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 36px;
    overflow: hidden;
    background: #070b14;
  }
  .login-orb {
    position: absolute;
    border-radius: 999px;
    filter: blur(12px);
    opacity: 0.55;
    pointer-events: none;
  }
  .orb-a {
    width: 520px;
    height: 520px;
    top: -180px;
    left: -120px;
    background: radial-gradient(circle, rgba(79, 70, 229, 0.65), transparent 65%);
  }
  .orb-b {
    width: 620px;
    height: 620px;
    right: -220px;
    bottom: -260px;
    background: radial-gradient(circle, rgba(13, 148, 136, 0.45), transparent 65%);
  }
  .login-grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(rgba(148, 163, 184, 0.06) 1px, transparent 1px),
      linear-gradient(90deg, rgba(148, 163, 184, 0.06) 1px, transparent 1px);
    background-size: 56px 56px;
    mask-image: radial-gradient(circle at 50% 45%, #000 20%, transparent 72%);
    pointer-events: none;
  }
  .login-card {
    position: relative;
    width: min(1040px, 100%);
    min-height: 640px;
    display: grid;
    grid-template-columns: 400px 1fr;
    border-radius: 28px;
    overflow: hidden;
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    animation: rise 0.5s ease-out both;
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(14px) scale(0.99); }
    to { opacity: 1; transform: none; }
  }
  .login-brand {
    position: relative;
    padding: 38px 36px;
    color: #e2e8f0;
    background:
      radial-gradient(circle at 20% 10%, rgba(99, 102, 241, 0.55), transparent 45%),
      linear-gradient(160deg, #0f172a 0%, #111b35 55%, #0b1220 100%);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .login-brand::after {
    content: "";
    position: absolute;
    width: 320px;
    height: 320px;
    right: -180px;
    bottom: -140px;
    border: 1px solid rgba(148, 163, 184, 0.16);
    border-radius: 999px;
    box-shadow:
      0 0 0 40px rgba(148, 163, 184, 0.04),
      0 0 0 90px rgba(148, 163, 184, 0.03);
  }
  .brand-lockup {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .brand-mark,
  .appbar-mark {
    width: 46px;
    height: 46px;
    border-radius: 14px;
    display: grid;
    place-items: center;
    font-size: 20px;
    font-weight: 800;
    color: #fff;
    background: linear-gradient(145deg, #6366f1, #0d9488);
    box-shadow: 0 10px 24px rgba(79, 70, 229, 0.35);
  }
  .brand-title {
    font-size: 18px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }
  .brand-caption {
    margin-top: 2px;
    font-size: 10px;
    letter-spacing: 0.2em;
    color: #94a3b8;
  }
  .brand-hero {
    margin-top: 54px;
  }
  .brand-hero h1 {
    margin: 0;
    font-size: 42px;
    line-height: 1.08;
    letter-spacing: -0.04em;
    color: #f8fafc;
  }
  .brand-hero p {
    margin: 16px 0 0;
    font-size: 13px;
    line-height: 1.8;
    color: #94a3b8;
    max-width: 280px;
  }
  .brand-features {
    margin-top: 44px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .feature {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .feature > span {
    font-size: 10px;
    font-weight: 700;
    color: #a5b4fc;
    letter-spacing: 0.1em;
    padding-top: 2px;
  }
  .feature div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .feature b {
    font-size: 12px;
    color: #e2e8f0;
    font-weight: 600;
  }
  .feature small {
    font-size: 11px;
    color: #64748b;
  }
  .brand-foot {
    margin-top: auto;
    color: #475569;
    font-size: 10px;
    letter-spacing: 0.06em;
  }

  .login-panel {
    padding: 38px 44px 28px;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .login-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
  }
  .login-head h2 {
    margin: 0;
    font-size: 25px;
    letter-spacing: -0.03em;
  }
  .login-head p {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 12px;
  }
  .secure-pill {
    flex: none;
    font-size: 10px;
    color: var(--accent);
    background: var(--accent-soft);
    border: 1px solid #99f6e4;
    border-radius: 999px;
    padding: 5px 9px;
    letter-spacing: 0.04em;
  }
  .segmented {
    margin-top: 24px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    padding: 4px;
    background: var(--surface-muted);
    border-radius: 12px;
  }
  .segmented button {
    height: 38px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
  }
  .segmented button.active {
    background: var(--surface);
    color: var(--primary);
    font-weight: 600;
    box-shadow: var(--shadow-sm);
  }

  .qr-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 18px 0 8px;
  }
  .qr-frame {
    position: relative;
    width: 234px;
    height: 234px;
    padding: 12px;
    border-radius: 18px;
    border: 1px solid var(--line);
    background: var(--surface-soft);
    display: grid;
    place-items: center;
  }
  .qr-frame img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    border-radius: 8px;
    background: #fff;
  }
  .qr-corners {
    position: absolute;
    inset: 6px;
    border-radius: 14px;
    pointer-events: none;
    background:
      linear-gradient(var(--primary), var(--primary)) left top / 18px 2px no-repeat,
      linear-gradient(var(--primary), var(--primary)) left top / 2px 18px no-repeat,
      linear-gradient(var(--primary), var(--primary)) right top / 18px 2px no-repeat,
      linear-gradient(var(--primary), var(--primary)) right top / 2px 18px no-repeat,
      linear-gradient(var(--primary), var(--primary)) left bottom / 18px 2px no-repeat,
      linear-gradient(var(--primary), var(--primary)) left bottom / 2px 18px no-repeat,
      linear-gradient(var(--primary), var(--primary)) right bottom / 18px 2px no-repeat,
      linear-gradient(var(--primary), var(--primary)) right bottom / 2px 18px no-repeat;
    opacity: 0.55;
  }
  .qr-loading {
    display: grid;
    justify-items: center;
    gap: 10px;
    color: var(--muted);
    font-size: 12px;
  }
  .spinner {
    width: 22px;
    height: 22px;
    border: 2px solid var(--line);
    border-top-color: var(--primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  .qr-title {
    margin-top: 6px;
    font-size: 15px;
    font-weight: 600;
  }
  .qr-status {
    min-height: 18px;
    color: var(--muted);
    font-size: 12px;
  }
  .qr-steps {
    margin-top: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted-2);
    font-size: 10px;
  }
  .qr-steps i {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--line-strong);
  }

  .step-strip {
    margin: 24px 0 18px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .step-strip > div {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted-2);
    font-size: 11px;
  }
  .step-strip b {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    border: 1px solid var(--line);
    font-size: 10px;
    font-weight: 700;
    background: var(--surface-soft);
    color: var(--muted);
  }
  .step-strip > div.active {
    color: var(--primary);
    font-weight: 600;
  }
  .step-strip > div.active b {
    background: var(--primary);
    border-color: var(--primary);
    color: #fff;
  }
  .step-strip > div.done b {
    background: var(--accent-soft);
    border-color: #99f6e4;
    color: var(--accent);
  }
  .step-strip > i {
    flex: 1;
    height: 1px;
    background: var(--line);
  }

  .auth-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    flex: 1;
  }
  .auth-form label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--ink-soft);
  }
  .auth-form label > span {
    font-weight: 600;
  }
  .auth-form input {
    height: 44px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    padding: 0 13px;
    font-size: 13px;
    color: var(--ink);
    outline: none;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .auth-form input:focus {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px rgba(79, 70, 229, 0.12);
  }
  .form-hint {
    margin: 0;
    color: var(--muted-2);
    font-size: 11px;
    line-height: 1.6;
  }
  .captcha-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface-soft);
  }
  .captcha-card img {
    height: 58px;
    flex: 1;
    object-fit: contain;
    border-radius: 6px;
    background: #fff;
  }
  .wx-banner {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 12px;
    border-radius: 12px;
    background: var(--accent-soft);
    border: 1px solid #99f6e4;
  }
  .wx-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 5px rgba(13, 148, 136, 0.12);
  }
  .wx-banner div:last-child {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .wx-banner b {
    font-size: 12px;
    color: #115e59;
  }
  .wx-banner span {
    font-size: 11px;
    color: #0f766e;
  }

  .btn {
    height: 42px;
    border-radius: 10px;
    border: 1px solid transparent;
    padding: 0 16px;
    font-size: 13px;
    font-weight: 600;
    transition: background 0.15s, border-color 0.15s, transform 0.1s;
  }
  .btn:active {
    transform: translateY(1px);
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .btn.primary {
    background: var(--primary);
    color: #fff;
  }
  .btn.primary:hover:not(:disabled) {
    background: var(--primary-hover);
  }
  .btn.ghost {
    background: var(--surface);
    border-color: var(--line-strong);
    color: var(--ink-soft);
  }
  .btn.ghost:hover {
    background: var(--surface-soft);
  }
  .btn.soft {
    background: var(--primary-soft);
    color: var(--primary-ink);
  }
  .btn.soft:hover {
    background: #e0e7ff;
  }
  .btn.wide {
    width: 100%;
  }
  .text-btn {
    border: 0;
    background: transparent;
    color: var(--primary);
    font-size: 11px;
    padding: 2px 4px;
    border-radius: 6px;
  }
  .text-btn:hover {
    background: var(--primary-soft);
  }
  .text-btn.light {
    color: #93c5fd;
  }
  .text-btn.light:hover {
    background: rgba(147, 197, 253, 0.1);
  }

  .login-foot {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
    color: var(--muted-2);
    font-size: 10px;
    line-height: 1.6;
  }

  .alert {
    border-radius: 10px;
    padding: 10px 12px;
    font-size: 12px;
    line-height: 1.5;
  }
  .alert.danger {
    background: var(--danger-soft);
    color: #991b1b;
    border: 1px solid #fecaca;
  }
  .alert.success {
    background: var(--success-soft);
    color: #166534;
    border: 1px solid #bbf7d0;
  }
  .page-alert {
    margin: 12px 18px 0;
  }

  .app-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .appbar {
    height: 64px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  .appbar-brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .appbar-mark {
    width: 36px;
    height: 36px;
    border-radius: 11px;
    font-size: 16px;
    box-shadow: none;
  }
  .appbar-brand > div:last-child {
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }
  .appbar-brand b {
    font-size: 14px;
    letter-spacing: 0.06em;
  }
  .appbar-brand span {
    font-size: 9px;
    color: var(--muted-2);
    letter-spacing: 0.16em;
  }
  .appbar-divider {
    width: 1px;
    height: 26px;
    background: var(--line);
    margin: 0 4px;
  }
  .term-picker {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .term-picker > span {
    font-size: 10px;
    color: var(--muted-2);
    letter-spacing: 0.08em;
    flex: none;
  }
  .term-picker select {
    height: 34px;
    max-width: 420px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-soft);
    color: var(--ink-soft);
    font-size: 12px;
    padding: 0 30px 0 10px;
    outline: none;
  }
  .term-picker select:focus {
    border-color: var(--primary);
  }
  .appbar-spacer {
    flex: 1;
  }
  .connection {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 11px;
  }
  .connection i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #22c55e;
    box-shadow: 0 0 0 4px rgba(34, 197, 94, 0.12);
  }
  .user-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 12px;
    border-left: 1px solid var(--line);
  }
  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 700;
    color: #fff;
    background: linear-gradient(145deg, #6366f1, #0d9488);
  }
  .user-text {
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .user-text b {
    font-size: 12px;
  }
  .user-text span {
    font-size: 10px;
    color: var(--muted-2);
  }
  .icon-btn {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--surface);
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .icon-btn:hover {
    color: var(--danger);
    border-color: #fecaca;
    background: var(--danger-soft);
  }
  .icon-btn svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .rail-btn.active,
  .rail-btn:hover {
    color: var(--primary);
    border-color: #c7d2fe;
    background: var(--primary-soft);
  }

  .workspace {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 286px 348px minmax(0, 1fr);
    gap: 14px;
    padding: 14px 18px 18px;
    height: calc(100vh - 64px);
    transition: grid-template-columns 0.2s ease;
  }
  .workspace.collapsed {
    grid-template-columns: 78px 348px minmax(0, 1fr);
  }
  .course-rail,
  .record-panel {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: var(--shadow-sm);
  }
  .rail-head,
  .panel-head {
    padding: 16px 16px 12px;
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
  .eyebrow,
  .stage-eyebrow {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.18em;
    color: var(--muted-2);
  }
  .rail-head h2,
  .panel-head h2 {
    margin: 4px 0 0;
    font-size: 19px;
    letter-spacing: -0.03em;
  }
  .rail-head p,
  .panel-head p {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--muted);
  }
  .count-pill {
    align-self: flex-start;
    min-width: 28px;
    height: 24px;
    border-radius: 999px;
    background: var(--primary-soft);
    color: var(--primary-ink);
    font-size: 11px;
    font-weight: 700;
    display: grid;
    place-items: center;
    padding: 0 8px;
  }
  .search-box {
    margin: 0 12px 10px;
    height: 36px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--surface-soft);
  }
  .search-box:focus-within {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px rgba(79, 70, 229, 0.08);
    background: var(--surface);
  }
  .search-box svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: var(--muted-2);
    stroke-width: 1.8;
    stroke-linecap: round;
    flex: none;
  }
  .search-box input {
    border: 0;
    outline: none;
    background: transparent;
    width: 100%;
    font-size: 12px;
    color: var(--ink);
  }
  .search-box input::placeholder {
    color: var(--muted-2);
  }

  .course-list,
  .record-list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .course-item,
  .record-item {
    border: 1px solid transparent;
    border-radius: 11px;
    background: transparent;
    padding: 9px;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    color: var(--ink);
    transition: background 0.12s, border-color 0.12s;
  }
  .course-item:hover,
  .record-item:hover {
    background: var(--surface-soft);
  }
  .course-item.active,
  .record-item.active {
    background: var(--primary-soft);
    border-color: #c7d2fe;
  }
  .course-avatar {
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    font-size: 13px;
    font-weight: 700;
    color: var(--primary-ink);
    background: linear-gradient(145deg, #eef2ff, #e0e7ff);
  }
  .course-item.active .course-avatar {
    color: #fff;
    background: linear-gradient(145deg, #6366f1, #4f46e5);
  }
  .course-copy,
  .record-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .course-copy b,
  .record-copy b {
    font-size: 12px;
    line-height: 1.35;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .course-copy span,
  .record-copy span {
    font-size: 10px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    color: var(--muted-2);
    font-size: 16px;
    flex: none;
  }
  .record-item {
    align-items: flex-start;
    padding: 10px 8px;
  }
  .record-item:disabled {
    opacity: 0.55;
  }
  .date-tile {
    flex: none;
    width: 38px;
    height: 44px;
    border-radius: 9px;
    background: var(--surface-muted);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
  }
  .record-item.active .date-tile {
    background: #fff;
  }
  .date-tile b {
    font-size: 15px;
    line-height: 1;
    color: var(--primary);
  }
  .date-tile span {
    font-size: 9px;
    color: var(--muted);
  }
  .record-badge {
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--primary-soft);
    color: var(--primary);
    font-size: 9px;
    margin-top: 4px;
  }
  .record-badge.muted {
    width: auto;
    border-radius: 999px;
    padding: 0 7px;
    background: var(--amber-soft);
    color: var(--amber);
    font-size: 9px;
  }
  .course-rail.collapsed .rail-copy,
  .course-rail.collapsed .search-box,
  .course-rail.collapsed .course-copy,
  .course-rail.collapsed .chev,
  .course-rail.collapsed .rail-foot,
  .course-rail.collapsed .count-pill {
    display: none;
  }
  .course-rail.collapsed .rail-head {
    padding: 14px 8px;
    justify-content: center;
  }
  .course-rail.collapsed .course-list {
    padding: 0 8px 8px;
    align-items: center;
  }
  .course-rail.collapsed .course-item {
    width: 100%;
    justify-content: center;
    padding: 9px;
  }
  .course-rail.collapsed .course-avatar {
    width: 38px;
    height: 38px;
  }
  .rail-foot {
    flex: none;
    padding: 10px 14px;
    border-top: 1px solid var(--line);
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    color: var(--muted-2);
  }
  .rail-foot b {
    color: var(--ink-soft);
    font-weight: 600;
  }
  .list-empty {
    padding: 24px 12px;
    text-align: center;
    color: var(--muted-2);
    font-size: 11px;
  }
  .course-skeleton,
  .record-skeleton {
    display: flex;
    gap: 10px;
    padding: 9px;
  }
  .course-skeleton > span,
  .record-skeleton > span {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--surface-muted);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .course-skeleton > div,
  .record-skeleton > div {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 4px;
  }
  .course-skeleton i,
  .record-skeleton i {
    display: block;
    height: 8px;
    border-radius: 99px;
    background: var(--surface-muted);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .course-skeleton i:last-child,
  .record-skeleton i:last-child {
    width: 55%;
  }
  @keyframes pulse {
    50% { opacity: 0.45; }
  }

  .stage {
    background:
      radial-gradient(circle at 80% 0%, rgba(79, 70, 229, 0.14), transparent 42%),
      var(--stage);
    border: 1px solid var(--stage-line);
    border-radius: var(--radius-lg);
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 18px;
    color: #e2e8f0;
    overflow: auto;
  }
  .stage-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 14px;
    margin-bottom: 14px;
  }
  .stage-eyebrow {
    color: #64748b;
  }
  .stage-head h2 {
    margin: 5px 0 0;
    font-size: 18px;
    letter-spacing: -0.02em;
    color: #f1f5f9;
  }
  .stage-head p {
    margin: 5px 0 0;
    font-size: 11px;
    color: #64748b;
  }
  .stage-tools {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .stage-btn {
    height: 30px;
    padding: 0 11px;
    border-radius: 8px;
    border: 1px solid #263247;
    background: #111a2e;
    color: #cbd5e1;
    font-size: 11px;
  }
  .stage-btn:hover {
    background: #172036;
    border-color: #334155;
  }
  .stage-loading,
  .stage-empty {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    text-align: center;
  }
  .stage-loading b,
  .stage-empty b {
    font-size: 14px;
    color: #e2e8f0;
  }
  .stage-loading span,
  .stage-empty span {
    max-width: 380px;
    font-size: 11px;
    line-height: 1.7;
    color: #64748b;
  }
  .play-orb {
    width: 70px;
    height: 70px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    border: 1px solid #263247;
    background: radial-gradient(circle at 30% 20%, #172036, #0d1424);
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.35);
    margin-bottom: 8px;
  }
  .play-orb svg {
    width: 24px;
    height: 24px;
    fill: #a5b4fc;
    margin-left: 3px;
  }
  .stage-tips {
    margin-top: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
    color: #475569;
    font-size: 10px;
  }
  .stage-tips i {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: #334155;
  }

  .video-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 12px;
  }
  .video-grid.two-videos {
    grid-template-columns: 1fr 1fr;
  }
  .video-card {
    min-width: 0;
    border: 1px solid #1e293b;
    border-radius: 12px;
    overflow: hidden;
    background: #0b1220;
  }
  .video-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: #0f1729;
    border-bottom: 1px solid #1e293b;
    font-size: 11px;
    color: #cbd5e1;
  }
  .video-head small {
    margin-left: auto;
    color: #64748b;
    font-size: 10px;
  }
  .video-body {
    position: relative;
    background: #000;
  }
  .video-card video {
    display: block;
    width: 100%;
    max-height: 380px;
    background: #000;
  }
  .video-buffer {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: rgba(2, 6, 23, 0.42);
    color: #cbd5e1;
    font-size: 11px;
    pointer-events: none;
    backdrop-filter: blur(1px);
  }
  .video-buffer .spinner {
    width: 18px;
    height: 18px;
    border-width: 2px;
    border-color: rgba(148, 163, 184, 0.3);
    border-top-color: #a5b4fc;
  }
  .video-missing {
    height: 190px;
    display: grid;
    place-items: center;
    color: #64748b;
    font-size: 11px;
  }
  .stage-alert {
    margin-top: 10px;
  }
  .subtitle-panel {
    margin-top: 12px;
    border: 1px solid #1e293b;
    border-radius: 12px;
    background: #0d1424;
    overflow: hidden;
  }
  .subtitle-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid #1e293b;
  }
  .subtitle-head > div:first-child {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .subtitle-head b {
    font-size: 12px;
    color: #e2e8f0;
  }
  .subtitle-count {
    font-size: 10px;
    color: #64748b;
  }
  .subtitle-actions {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .subtitle-scroll {
    position: relative;
    height: 154px;
    overflow: auto;
    padding: 6px 8px;
    scroll-behavior: smooth;
  }
  .cue-line {
    width: 100%;
    display: flex;
    gap: 10px;
    text-align: left;
    border: 0;
    border-left: 2px solid transparent;
    background: transparent;
    padding: 6px 8px;
    border-radius: 6px;
    color: #94a3b8;
    font-size: 11px;
    line-height: 1.65;
  }
  .cue-line:hover {
    background: #111a2e;
  }
  .cue-line.active {
    background: rgba(99, 102, 241, 0.14);
    border-left-color: #818cf8;
    color: #f1f5f9;
  }
  .cue-time {
    flex: none;
    width: 42px;
    padding-top: 1px;
    color: #64748b;
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 10px;
  }
  .cue-line.active .cue-time {
    color: #a5b4fc;
  }
  .cue-text {
    flex: 1;
  }
  .subtitle-empty {
    display: grid;
    place-items: center;
    height: 140px;
    color: #64748b;
    font-size: 11px;
  }
  .stage-info {
    margin-top: 12px;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  .stage-info > div {
    min-width: 0;
    padding: 9px 10px;
    border: 1px solid #1e293b;
    border-radius: 10px;
    background: #0d1424;
  }
  .stage-info span {
    display: block;
    font-size: 9px;
    letter-spacing: 0.08em;
    color: #64748b;
  }
  .stage-info b {
    display: block;
    margin-top: 3px;
    font-size: 11px;
    font-weight: 500;
    color: #cbd5e1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stage-info .mono {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 10px;
  }
  .stage-footer {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid #1e293b;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .subtitle-state {
    display: flex;
    align-items: center;
    gap: 7px;
    color: #94a3b8;
    font-size: 10px;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #475569;
  }
  .dot.success { background: #22c55e; }
  .dot.danger { background: #ef4444; }
  .stage-actions {
    display: flex;
    gap: 4px;
  }
  .muted {
    color: var(--muted-2);
  }

  @media (max-height: 760px) {
    .login-screen {
      padding: 18px;
    }
    .login-card {
      min-height: auto;
    }
    .brand-hero {
      margin-top: 26px;
    }
    .brand-hero h1 {
      font-size: 34px;
    }
    .brand-features {
      margin-top: 22px;
      gap: 9px;
    }
    .login-panel {
      padding: 26px 34px 20px;
    }
    .qr-frame {
      width: 200px;
      height: 200px;
    }
  }

  @media (max-width: 1180px) {
    .workspace {
      grid-template-columns: 250px 310px minmax(0, 1fr);
    }
    .video-grid.two-videos {
      grid-template-columns: 1fr;
    }
    .stage-info {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .term-picker select {
      max-width: 300px;
    }
  }
</style>
