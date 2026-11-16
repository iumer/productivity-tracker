import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  CategoryRule,
  CurrentActivity,
  DaySummary,
  SoftNudgeBanner,
  TaskInstance,
  TaskTemplate,
  TemplateDraft,
  TodaySnapshot,
} from "./types";

export const api = {
  getToday: () => invoke<TodaySnapshot>("get_today"),
  setTrackingEnabled: (enabled: boolean) =>
    invoke<TodaySnapshot>("set_tracking_enabled", { enabled }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<void>("save_settings", { settings }),
  getCategoryRules: () => invoke<CategoryRule[]>("get_category_rules"),
  saveCategoryRules: (rules: CategoryRule[]) =>
    invoke<void>("save_category_rules", { rules }),
  getTemplates: () => invoke<TaskTemplate[]>("get_templates"),
  saveDayTemplates: (weekday: number, templates: TemplateDraft[]) =>
    invoke<void>("save_day_templates", { weekday, templates }),
  updateTask: (task: TaskInstance) => invoke<void>("update_task", { task }),
  addTask: (summary: string, ticketLink?: string, notes?: string) =>
    invoke<TaskInstance>("add_task", { summary, ticketLink, notes }),
  deleteTask: (id: string) => invoke<void>("delete_task", { id }),
  getCurrentActivity: () => invoke<CurrentActivity>("get_current_activity"),
  openAccessibilityPrefs: () => invoke<void>("open_accessibility_prefs"),
  openAutomationPrefs: () => invoke<void>("open_automation_prefs"),
  checkAccessibility: () => invoke<boolean>("check_accessibility"),
  checkAutomation: () =>
    invoke<"ok" | "denied" | "unknown">("check_automation"),
  previewSoftNudge: () => invoke<SoftNudgeBanner>("preview_soft_nudge"),
  dismissSoftNudge: () => invoke<void>("dismiss_soft_nudge"),
  getDaySummary: (date: string) =>
    invoke<DaySummary>("get_day_summary", { date }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
};
