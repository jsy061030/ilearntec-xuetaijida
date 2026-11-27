import { invoke } from "@tauri-apps/api/core";

export interface User {
  id: string;
  username: string;
  realname: string;
  schoolId?: string | null;
  schoolName?: string | null;
}

export interface Term {
  id: string;
  year: string;
  num: string;
  name: string;
  selected?: string | null;
  startDate?: string | null;
  endDate?: string | null;
}

export interface Lesson {
  id: string;
  name?: string | null;
  courseId?: string | null;
  courseName?: string | null;
  teacherId?: string | null;
  teacherName?: string | null;
  className?: string | null;
  termId?: string | null;
  termYear?: string | null;
  term?: string | null;
  statusName?: string | null;
  cover?: string | null;
}

export interface VideoClass {
  videoClassId: string;
  videoName: string;
}

export interface RecordItem {
  id: string;
  resourceId: string;
  liveRecordName?: string | null;
  courseName?: string | null;
  teacherName?: string | null;
  buildingName?: string | null;
  roomName?: string | null;
  currentWeek?: string | null;
  currentDay?: string | null;
  currentDate?: string | null;
  section?: string | null;
  timeRange?: string | null;
  liveStatus?: string | null;
  videoPath?: string | null;
  videoClassMap?: VideoClass[];
}

export interface Video {
  id?: string | null;
  videoCode?: string | null;
  videoName?: string | null;
  videoPath?: string | null;
  videoSize?: string | null;
}

export interface VideoResource {
  resourceName?: string | null;
  phaseUrl?: string | null;
  videoList: Video[];
  teacherName?: string | null;
  roomName?: string | null;
  buildingName?: string | null;
  classNames?: string | null;
  classType?: number | null;
  resourceType?: number | null;
}

export interface QrStart {
  image: string;
}

export interface QrStatus {
  status: "pending" | "expired" | "scanned" | string;
  user?: User | null;
}

export interface PasswordStart {
  needSecondFactor: boolean;
  user?: User | null;
  captcha?: string | null;
}

export const api = {
  getProxyBase: () => invoke<string>("get_proxy_base"),
  loadSession: () => invoke<User | null>("load_session"),
  startQr: () => invoke<QrStart>("start_qr"),
  pollQr: () => invoke<QrStatus>("poll_qr"),
  startPassword: (username: string, password: string) =>
    invoke<PasswordStart>("start_password", { username, password }),
  refreshCaptcha: () => invoke<string>("refresh_captcha"),
  requestWx: (captcha: string) => invoke<string>("request_wx", { captcha }),
  submitWx: (captcha: string, wxCode: string) =>
    invoke<User>("submit_wx", { captcha, wxCode }),
  logout: () => invoke<void>("logout"),
  getTerms: () => invoke<Term[]>("get_terms"),
  getLessons: (termYear: string, term: string) =>
    invoke<Lesson[]>("get_lessons", { termYear, term }),
  getRecords: (termId: string, lessonId: string) =>
    invoke<RecordItem[]>("get_records", { termId, lessonId }),
  getVideo: (resourceId: string) =>
    invoke<VideoResource>("get_video", { resourceId }),
  getSubtitle: (url: string) => invoke<string>("get_subtitle", { url }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export function humanSize(s?: string | null): string {
  if (!s) return "";
  const n = Number(s);
  if (!Number.isFinite(n)) return s;
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(2)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${n} B`;
}
