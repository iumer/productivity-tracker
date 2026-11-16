export type AppTheme = "legacy" | "liquidGlass";

export type GlassPrefs = {
  /** 0 = clear / glassy, 100 = frosted (macOS 27 Liquid Glass slider). */
  glassLevel: number;
  glassReduceTransparency: boolean;
  glassIncreaseContrast: boolean;
};

export type AppSettings = {
  idleThresholdMinutes: number;
  dayEnabled: boolean[];
  accessibilityPrompted: boolean;
  onboardingCompleted?: boolean;
  trackingEnabled: boolean;
  softNudgesEnabled: boolean;
  appTheme?: AppTheme;
  glassLevel?: number;
  glassReduceTransparency?: boolean;
  glassIncreaseContrast?: boolean;
  /** @deprecated use appTheme */
  softNudgeStyle?: "classic" | "liquidGlass";
};

export type SoftNudgeStyle = "classic" | "liquidGlass";

export type CategoryRule = {
  id: string;
  label: string;
  display: string;
  matchApps: string[];
  titleContains: string[];
  urlContains: string[];
  priority: number;
  enabled: boolean;
};

export type TaskTemplate = {
  id: string;
  weekday: number;
  summary: string;
  sortOrder: number;
  reminderEnabled: boolean;
  reminderHour: number | null;
  reminderMinute: number | null;
  reminderTz: string;
};

export type TemplateDraft = {
  summary: string;
  reminderEnabled: boolean;
  reminderHour: number | null;
  reminderMinute: number | null;
  reminderTz: string;
};

export type TaskInstance = {
  id: string;
  date: string;
  templateId: string | null;
  summary: string;
  ticketLink: string;
  notes: string;
  status: string;
  sortOrder: number;
  reminderEnabled: boolean;
  reminderHour: number | null;
  reminderMinute: number | null;
  reminderTz: string;
  reminderFired: boolean;
  softNudgeFired: boolean;
};

export type CategoryTime = {
  categoryId: string;
  categoryLabel: string;
  seconds: number;
};

export type DaySummary = {
  date: string;
  windowStart: string;
  windowEnd: string;
  activeSeconds: number;
  idleSeconds: number;
  productiveSeconds: number;
  entertainmentSeconds: number;
  communicationSeconds: number;
  browsingSeconds: number;
  otherSeconds: number;
  byCategory: CategoryTime[];
};

export type CurrentActivity = {
  appName: string;
  windowTitle: string;
  categoryId: string;
  categoryLabel: string;
  display: string;
  isIdle: boolean;
  idleSeconds: number;
  sessionStartedAt: string | null;
  trackingEnabled: boolean;
  /** Why collection is paused while tracking stays ON (e.g. Display off, Locked). */
  pauseReason?: string | null;
};

export type ZoneClock = {
  label: string;
  date: string;
  time: string;
  weekday: string;
};

export type DualClocks = {
  pakistan: ZoneClock;
  usCentral: ZoneClock;
  workDayLabel: string;
  workDayWeekday: number;
};

export type ActivityLogEntry = {
  id: string;
  kind: "start" | "stop" | "activity" | string;
  startedAt: string;
  endedAt: string | null;
  label: string;
  detail: string;
};

export type SoftNudgeBanner = {
  summary: string;
  message: string;
  at: string;
  /** `todo` (gentle nudge) or `alarmHeadsUp` (hard-alarm heads-up). */
  kind?: "todo" | "alarmHeadsUp" | string;
  /** Formatted alarm time for alarmHeadsUp, e.g. "2:30 PM CT". */
  alarmTime?: string | null;
  style?: SoftNudgeStyle;
  glassLevel?: number;
  glassReduceTransparency?: boolean;
  glassIncreaseContrast?: boolean;
};

export type TodaySnapshot = {
  date: string;
  clocks: DualClocks;
  tasks: TaskInstance[];
  summary: DaySummary;
  current: CurrentActivity;
  tasksDone: number;
  tasksTotal: number;
  trackingEnabled: boolean;
  activityLog: ActivityLogEntry[];
  softNudge: SoftNudgeBanner | null;
};

export const DAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;

export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${sec}s`;
}

export function formatHm12(hour: number, minute: number): string {
  const h12 = hour % 12 === 0 ? 12 : hour % 12;
  const ampm = hour < 12 ? "AM" : "PM";
  return `${h12}:${String(minute).padStart(2, "0")} ${ampm}`;
}

/** Alarms are always US Central; show PK equivalent for clarity. */
export function formatReminderLabel(
  hour: number | null | undefined,
  minute: number | null | undefined,
  _tz?: string | null,
): string {
  if (hour == null || minute == null) return "";
  const { hour: pkH, minute: pkM, nextCalendarDay } = ctToPk(hour, minute);
  const pkExtra = nextCalendarDay ? " (+1 PK day)" : "";
  return `${formatHm12(hour, minute)} CT ≈ ${formatHm12(pkH, pkM)} PK${pkExtra}`;
}

/** Rough CT→PK conversion using current Chicago DST. */
export function ctToPk(
  hour: number,
  minute: number,
): { hour: number; minute: number; nextCalendarDay: boolean } {
  const sample = new Date();
  const short = new Intl.DateTimeFormat("en-US", {
    timeZone: "America/Chicago",
    timeZoneName: "short",
  })
    .formatToParts(sample)
    .find((p) => p.type === "timeZoneName")?.value;
  // CDT (UTC-5): PK is +10h; CST (UTC-6): PK is +11h
  const offset = short === "CDT" ? 10 : 11;
  let h = hour + offset;
  let nextCalendarDay = false;
  if (h >= 24) {
    h -= 24;
    nextCalendarDay = true;
  }
  return { hour: h, minute, nextCalendarDay };
}

/** Format ISO timestamp in Pakistan time (work-day timezone). */
export function formatLogTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return new Intl.DateTimeFormat("en-US", {
    timeZone: "Asia/Karachi",
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
    hour12: true,
  }).format(d);
}
