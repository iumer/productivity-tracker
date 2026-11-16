use crate::activity::{
    accessibility_likely_granted, alert_task_reminder, browser_automation_status,
    collection_pause_reason, frontmost_app, is_our_app, notify_task_reminder,
    open_accessibility_settings, open_automation_settings, system_idle_seconds, AutomationStatus,
    PauseReason,
};
use crate::classifier::classify;
use crate::db::{
    dual_clocks, format_reminder_time_label, now_iso, reminder_instant_in_work_day,
    sync_soft_nudge_plan, today_date, ActivityLogEntry, ActivitySession, AppSettings,
    CategoryRule, DaySummary, Db, DualClocks, SoftNudgeEligible, TaskInstance, TaskTemplate,
};
use crate::seed::{default_category_rules, default_task_templates};
use crate::{AppState, SoftNudgeBanner};
use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, State,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentActivity {
    pub app_name: String,
    pub window_title: String,
    pub category_id: String,
    pub category_label: String,
    pub display: String,
    pub is_idle: bool,
    pub idle_seconds: f64,
    pub session_started_at: Option<String>,
    pub tracking_enabled: bool,
    /// Why collection is paused while tracking preference stays ON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodaySnapshot {
    pub date: String,
    pub clocks: DualClocks,
    pub tasks: Vec<TaskInstance>,
    pub summary: DaySummary,
    pub current: CurrentActivity,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub tracking_enabled: bool,
    pub activity_log: Vec<ActivityLogEntry>,
    pub soft_nudge: Option<SoftNudgeBanner>,
}

fn with_db<T, F>(state: &AppState, f: F) -> Result<T, String>
where
    F: FnOnce(&Db) -> Result<T, rusqlite::Error>,
{
    let db = state.db.lock();
    f(&db).map_err(|e| e.to_string())
}

pub fn ensure_seeded(db: &Db) -> Result<(), String> {
    let default_rules = default_category_rules();
    if db.count_category_rules().map_err(|e| e.to_string())? == 0 {
        for rule in &default_rules {
            db.insert_category_rule(rule).map_err(|e| e.to_string())?;
        }
    } else {
        // Refresh matchers for known rules so tab detection stays up to date.
        for rule in &default_rules {
            db.upsert_category_rule_meta(rule)
                .map_err(|e| e.to_string())?;
        }
    }
    if db.count_templates().map_err(|e| e.to_string())? == 0 {
        for t in default_task_templates() {
            db.insert_template(&t).map_err(|e| e.to_string())?;
        }
    }
    let mut settings = db.get_settings().map_err(|e| e.to_string())?;
    // Don't force onboarding on upgrades — only brand-new installs.
    if !settings.onboarding_completed
        && (settings.accessibility_prompted || db.has_any_activity().unwrap_or(false))
    {
        settings.onboarding_completed = true;
        let _ = db.save_settings(&settings);
    }
    let _ = db.patch_known_reminders().map_err(|e| e.to_string())?;
    // Split historical Other rows that already have an app name into named entries.
    let _ = db
        .reclassify_named_other_sessions()
        .map_err(|e| e.to_string())?;
    // Helpers like app_mode_loader / Error Reporting stay under Other.
    let _ = db
        .fold_noise_apps_into_other()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn activity_stopped() -> CurrentActivity {
    CurrentActivity {
        app_name: String::new(),
        window_title: String::new(),
        category_id: "stopped".into(),
        category_label: "Stopped".into(),
        display: "Tracking stopped".into(),
        is_idle: false,
        idle_seconds: 0.0,
        session_started_at: None,
        tracking_enabled: false,
        pause_reason: None,
    }
}

fn activity_paused(reason: PauseReason) -> CurrentActivity {
    CurrentActivity {
        app_name: String::new(),
        window_title: String::new(),
        category_id: "paused".into(),
        category_label: "Paused".into(),
        display: reason.label().into(),
        is_idle: false,
        idle_seconds: 0.0,
        session_started_at: None,
        tracking_enabled: true,
        pause_reason: Some(reason.short().into()),
    }
}

/// Enter auto-pause: clip open session + end tracking run (clean boundary).
fn enter_auto_pause(state: &AppState, reason: PauseReason) -> Result<CurrentActivity, String> {
    let now = now_iso();
    with_db(state, |db| {
        let _ = db.close_all_open_sessions(&now);
        if let Some(run_id) = state.current_run_id.lock().take() {
            let _ = db.end_tracking_run(&run_id, &now);
        }
        Ok(())
    })?;
    *state.current_session_id.lock() = None;
    *state.last_fingerprint.lock() = Some(format!("paused:{}", reason.short()));
    *state.auto_paused.lock() = true;
    *state.last_tick_instant.lock() = Some(std::time::Instant::now());
    *state.last_tick_wall.lock() = Some(now.clone());
    let _ = with_db(state, |db| db.set_heartbeat(&now));
    Ok(activity_paused(reason))
}

/// Leave auto-pause: start a fresh tracking run if preference is still ON.
fn leave_auto_pause_if_needed(state: &AppState) -> Result<(), String> {
    if !*state.auto_paused.lock() {
        return Ok(());
    }
    *state.auto_paused.lock() = false;
    let settings = with_db(state, |db| db.get_settings())?;
    if settings.tracking_enabled && state.current_run_id.lock().is_none() {
        let now = now_iso();
        let run = with_db(state, |db| db.start_tracking_run(&now))?;
        *state.current_run_id.lock() = Some(run.id);
        *state.last_fingerprint.lock() = None;
    }
    Ok(())
}

pub fn tick_collector(state: &AppState) -> Result<CurrentActivity, String> {
    let settings = with_db(state, |db| db.get_settings())?;

    if !settings.tracking_enabled {
        // Close any open session and skip polling apps while stopped.
        let now = now_iso();
        let _ = with_db(state, |db| {
            db.close_all_open_sessions(&now)?;
            if let Some(run_id) = state.current_run_id.lock().take() {
                let _ = db.end_tracking_run(&run_id, &now);
            }
            Ok(())
        });
        *state.current_session_id.lock() = None;
        *state.last_fingerprint.lock() = Some("stopped".into());
        *state.auto_paused.lock() = false;
        // Keep last_tick_* so quit/sleep heal still has a good anchor.
        return Ok(activity_stopped());
    }

    // Sleep / suspend: clip any open session at the last awake tick (no idle fill).
    heal_sleep_gap(state)?;

    // Display asleep or screen locked → auto-pause (clamshell with awake display still tracks).
    if let Some(reason) = collection_pause_reason() {
        return enter_auto_pause(state, reason);
    }
    leave_auto_pause_if_needed(state)?;

    // Ensure a tracking run exists while collecting.
    if state.current_run_id.lock().is_none() {
        let now = now_iso();
        if let Ok(run) = with_db(state, |db| db.start_tracking_run(&now)) {
            *state.current_run_id.lock() = Some(run.id);
        }
    }

    let rules = with_db(state, |db| db.list_category_rules())?;
    let idle_secs = system_idle_seconds();
    let threshold = (settings.idle_threshold_minutes as f64) * 60.0;
    let is_idle = idle_secs >= threshold;

    let mut front = if is_idle {
        crate::activity::FrontApp::default()
    } else {
        frontmost_app()
    };

    // Don't pollute stats when the tracker window is focused.
    let focusing_self = is_our_app(&front.app_name);
    if focusing_self {
        front = crate::activity::FrontApp {
            app_name: "Productivity Tracker".into(),
            window_title: String::new(),
            page_url: String::new(),
        };
    }

    let app_name = front.app_name.clone();
    let classified = if is_idle {
        crate::classifier::Classified {
            category_id: "idle".into(),
            category_label: "Idle".into(),
            display: "Idle".into(),
        }
    } else if focusing_self {
        crate::classifier::Classified {
            category_id: "other".into(),
            category_label: "Other".into(),
            display: "Tracker".into(),
        }
    } else if app_name.is_empty() {
        // Accessibility missing / unreadable frontmost — not Idle.
        crate::classifier::Classified {
            category_id: "unknown".into(),
            category_label: "Unknown".into(),
            display: "Unknown — check Accessibility".into(),
        }
    } else {
        let c = classify(&app_name, &front.window_title, &front.page_url, &rules);
        if c.category_id.starts_with("app-") {
            let _ = with_db(state, |db| db.ensure_auto_app_rule(&app_name));
        }
        c
    };

    let fingerprint = format!(
        "{}|{}|{}|{}|{}",
        classified.category_id,
        app_name,
        front.window_title,
        front.page_url,
        is_idle
    );

    let mut last_fp = state.last_fingerprint.lock();

    if last_fp.as_deref() != Some(fingerprint.as_str()) {
        let now = now_iso();
        with_db(state, |db| {
            // Always close every open row first — memory can lose track after restarts,
            // and orphaned open sessions inflate "Time today" massively.
            let _ = db.close_all_open_sessions(&now);
            let id = Uuid::new_v4().to_string();
            let session = ActivitySession {
                id: id.clone(),
                started_at: now.clone(),
                ended_at: None,
                app_name: app_name.clone(),
                window_title: front.window_title.clone(),
                category_id: classified.category_id.clone(),
                category_label: classified.category_label.clone(),
                is_idle,
                run_id: state.current_run_id.lock().clone(),
            };
            db.open_session(&session)?;
            Ok(id)
        })
        .map(|id| {
            *state.current_session_id.lock() = Some(id);
        })?;
        *last_fp = Some(fingerprint);
    }

    // Record this tick so the next wake can clip sleep time correctly.
    let tick_iso = now_iso();
    *state.last_tick_instant.lock() = Some(std::time::Instant::now());
    *state.last_tick_wall.lock() = Some(tick_iso.clone());
    let _ = with_db(state, |db| db.set_heartbeat(&tick_iso));

    Ok(CurrentActivity {
        app_name,
        window_title: front.window_title,
        category_id: classified.category_id,
        category_label: classified.category_label,
        display: classified.display,
        is_idle,
        idle_seconds: idle_secs,
        session_started_at: with_db(state, |db| {
            Ok(db.get_open_session()?.map(|s| s.started_at))
        })?,
        tracking_enabled: true,
        pause_reason: None,
    })
}

/// If the collector missed ticks (sleep / suspend), close the pre-sleep session at
/// the last tick time. Sleep time is left **uncounted** (not idle, not active).
fn heal_sleep_gap(state: &AppState) -> Result<(), String> {
    /// Poll is 5s; anything over ~2 minutes without a tick is sleep/suspend, not lag.
    const GAP_SECS: u64 = 120;

    let prev_instant = *state.last_tick_instant.lock();
    let Some(prev_instant) = prev_instant else {
        return Ok(());
    };
    let gap = prev_instant.elapsed();
    if gap.as_secs() < GAP_SECS {
        return Ok(());
    }

    let Some(gap_start) = state.last_tick_wall.lock().clone() else {
        return Ok(());
    };

    with_db(state, |db| {
        // Stop the last app at the moment we last knew the Mac was awake / lid-open.
        let _ = db.close_all_open_sessions(&gap_start);
        Ok(())
    })?;

    *state.current_session_id.lock() = None;
    *state.last_fingerprint.lock() = None;
    Ok(())
}

#[tauri::command]
pub fn get_today(state: State<'_, AppState>) -> Result<TodaySnapshot, String> {
    let date = today_date();
    let tasks = with_db(&state, |db| db.ensure_today_tasks())?;
    let summary = with_db(&state, |db| db.day_summary(&date))?;
    // Single tick — background loop already samples when UI is idle.
    let current = tick_collector(&state)?;
    let tasks_done = tasks.iter().filter(|t| t.status == "done").count();
    let tasks_total = tasks.len();
    let tracking_enabled = current.tracking_enabled;
    let activity_log = with_db(&state, |db| db.activity_log_for_date(&date)).unwrap_or_default();
    let soft_nudge = state.last_soft_nudge.lock().clone();
    Ok(TodaySnapshot {
        date,
        clocks: dual_clocks(),
        tasks,
        summary,
        current,
        tasks_done,
        tasks_total,
        tracking_enabled,
        activity_log,
        soft_nudge,
    })
}

#[tauri::command]
pub fn set_tracking_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<TodaySnapshot, String> {
    set_tracking_enabled_inner(&app, &state, enabled)
}

#[tauri::command]
pub fn complete_onboarding(state: State<'_, AppState>) -> Result<(), String> {
    let mut settings = with_db(&state, |db| db.get_settings())?;
    settings.onboarding_completed = true;
    settings.accessibility_prompted = true;
    with_db(&state, |db| db.save_settings(&settings))?;
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    with_db(&state, |db| db.get_settings())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    with_db(&state, |db| db.save_settings(&settings))
}

#[tauri::command]
pub fn get_category_rules(state: State<'_, AppState>) -> Result<Vec<CategoryRule>, String> {
    with_db(&state, |db| db.list_category_rules())
}

#[tauri::command]
pub fn save_category_rules(
    state: State<'_, AppState>,
    rules: Vec<CategoryRule>,
) -> Result<(), String> {
    with_db(&state, |db| db.save_category_rules(&rules))
}

#[tauri::command]
pub fn get_templates(state: State<'_, AppState>) -> Result<Vec<TaskTemplate>, String> {
    with_db(&state, |db| db.list_templates())
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDraft {
    pub summary: String,
    #[serde(default)]
    pub reminder_enabled: bool,
    #[serde(default)]
    pub reminder_hour: Option<u32>,
    #[serde(default)]
    pub reminder_minute: Option<u32>,
    #[serde(default = "default_draft_tz")]
    pub reminder_tz: String,
}

fn default_draft_tz() -> String {
    "America/Chicago".into()
}

#[tauri::command]
pub fn save_day_templates(
    app: AppHandle,
    state: State<'_, AppState>,
    weekday: i32,
    templates: Vec<TemplateDraft>,
) -> Result<(), String> {
    let built: Vec<TaskTemplate> = templates
        .into_iter()
        .enumerate()
        .filter(|(_, t)| !t.summary.trim().is_empty())
        .map(|(i, t)| {
            let enabled = t.reminder_enabled && t.reminder_hour.is_some() && t.reminder_minute.is_some();
            TaskTemplate {
                id: Uuid::new_v4().to_string(),
                weekday,
                summary: t.summary.trim().to_string(),
                sort_order: i as i32,
                reminder_enabled: enabled,
                reminder_hour: if enabled { t.reminder_hour } else { None },
                reminder_minute: if enabled { t.reminder_minute } else { None },
                reminder_tz: "America/Chicago".into(),
            }
        })
        .collect();
    with_db(&state, |db| db.replace_templates_for_weekday(weekday, &built))?;
    update_tray(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn update_task(state: State<'_, AppState>, task: TaskInstance) -> Result<(), String> {
    with_db(&state, |db| {
        let mut task = task;
        db.update_task(&task)?;
        // Alarm / title edits on Today also update the weekday template.
        db.mirror_task_to_weekday_template(&mut task)?;
        Ok(())
    })
}

#[tauri::command]
pub fn add_task(
    state: State<'_, AppState>,
    summary: String,
    ticket_link: Option<String>,
    notes: Option<String>,
) -> Result<TaskInstance, String> {
    let date = today_date();
    let existing = with_db(&state, |db| db.list_tasks_for_date(&date))?;
    let mut task = TaskInstance {
        id: Uuid::new_v4().to_string(),
        date,
        template_id: None,
        summary: summary.trim().to_string(),
        ticket_link: ticket_link.unwrap_or_default(),
        notes: notes.unwrap_or_default(),
        status: "todo".into(),
        sort_order: existing.len() as i32,
        reminder_enabled: false,
        reminder_hour: None,
        reminder_minute: None,
        reminder_tz: "America/Chicago".into(),
        reminder_fired: false,
        soft_nudge_fired: false,
    };
    with_db(&state, |db| {
        db.insert_task(&task)?;
        // Remember on this weekday's template list for future days.
        db.mirror_task_to_weekday_template(&mut task)?;
        Ok(())
    })?;
    Ok(task)
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_db(&state, |db| {
        let date = today_date();
        let tasks = db.list_tasks_for_date(&date)?;
        if let Some(task) = tasks.into_iter().find(|t| t.id == id) {
            if let Some(tid) = task.template_id {
                // Removing from Today also drops it from that weekday's recurring list.
                let _ = db.delete_template(&tid);
            }
        }
        db.delete_task(&id)
    })
}

#[tauri::command]
pub fn get_current_activity(state: State<'_, AppState>) -> Result<CurrentActivity, String> {
    tick_collector(&state)
}

#[tauri::command]
pub fn open_accessibility_prefs() -> Result<(), String> {
    open_accessibility_settings();
    Ok(())
}

#[tauri::command]
pub fn open_automation_prefs() -> Result<(), String> {
    open_automation_settings();
    Ok(())
}

#[tauri::command]
pub fn check_accessibility() -> Result<bool, String> {
    Ok(accessibility_likely_granted())
}

#[tauri::command]
pub fn check_automation() -> Result<String, String> {
    Ok(match browser_automation_status() {
        AutomationStatus::Ok => "ok".into(),
        AutomationStatus::Denied => "denied".into(),
        AutomationStatus::Unknown => "unknown".into(),
    })
}

#[tauri::command]
pub fn get_day_summary(state: State<'_, AppState>, date: String) -> Result<DaySummary, String> {
    with_db(&state, |db| db.day_summary(&date))
}

pub fn tray_title_from_snapshot(snap: &TodaySnapshot) -> String {
    if !snap.tracking_enabled {
        return "Stopped".into();
    }
    if snap.current.category_id == "paused" {
        let why = snap
            .current
            .pause_reason
            .as_deref()
            .unwrap_or("Paused");
        return format!("Paused · {why}");
    }
    let progress = if snap.tasks_total > 0 {
        format!("{}/{}", snap.tasks_done, snap.tasks_total)
    } else {
        "–".into()
    };
    let short = if snap.current.is_idle {
        "Idle".to_string()
    } else {
        snap.current
            .display
            .chars()
            .take(18)
            .collect::<String>()
    };
    format!("{progress} · {short}")
}

fn format_hm(secs: i64) -> String {
    let s = secs.max(0);
    let h = s / 3600;
    let m = (s % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let snap = get_today_inner(&state).ok();
    let tracking_on = snap
        .as_ref()
        .map(|s| s.tracking_enabled)
        .unwrap_or(false);
    let totals_label = snap
        .as_ref()
        .map(|s| {
            format!(
                "Today · Active {} · Idle {}",
                format_hm(s.summary.active_seconds),
                format_hm(s.summary.idle_seconds)
            )
        })
        .unwrap_or_else(|| "Today · —".into());
    let toggle_label = if tracking_on {
        "Stop Tracking"
    } else {
        "Start Tracking"
    };

    let totals = MenuItem::with_id(app, "totals", totals_label, false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle_tracking", toggle_label, true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Open Tracker", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    Menu::with_items(app, &[&totals, &toggle, &sep, &show, &quit])
}

pub fn refresh_tray_menu(app: &AppHandle) {
    if let Ok(menu) = build_tray_menu(app) {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

pub fn set_tracking_enabled_inner(
    app: &AppHandle,
    state: &AppState,
    enabled: bool,
) -> Result<TodaySnapshot, String> {
    let mut settings = with_db(state, |db| db.get_settings())?;
    settings.tracking_enabled = enabled;
    with_db(state, |db| db.save_settings(&settings))?;

    let now = now_iso();
    if enabled {
        *state.auto_paused.lock() = false;
        let run = with_db(state, |db| db.start_tracking_run(&now))?;
        *state.current_run_id.lock() = Some(run.id);
        *state.last_fingerprint.lock() = None;
    } else {
        let _ = with_db(state, |db| {
            db.close_all_open_sessions(&now)?;
            Ok(())
        });
        *state.current_session_id.lock() = None;
        if let Some(run_id) = state.current_run_id.lock().take() {
            let _ = with_db(state, |db| db.end_tracking_run(&run_id, &now));
        }
        *state.last_fingerprint.lock() = Some("stopped".into());
        *state.auto_paused.lock() = false;
    }

    let _ = tick_collector(state)?;
    let snap = get_today_inner(state)?;
    let title = tray_title_from_snapshot(&snap);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&title));
        let _ = tray.set_title(Some(&title));
    }
    refresh_tray_menu(app);
    let _ = app.emit("tracking-changed", enabled);
    Ok(snap)
}

#[allow(dead_code)]
pub fn update_tray(app: &AppHandle, state: &AppState) {
    // Heartbeat even when tracking is off (soft nudges + quit heal).
    let tick_iso = now_iso();
    *state.last_tick_instant.lock() = Some(std::time::Instant::now());
    *state.last_tick_wall.lock() = Some(tick_iso.clone());
    let _ = with_db(state, |db| db.set_heartbeat(&tick_iso));

    if let Ok(snap) = get_today_inner(state) {
        let title = tray_title_from_snapshot(&snap);
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(&title));
            let _ = tray.set_title(Some(&title));
        }
        refresh_tray_menu(app);
    }
    check_and_fire_reminders(app, state);
    check_and_fire_soft_nudges(app, state);
}

/// Graceful shutdown: end open sessions at last heartbeat so quit doesn't invent hours.
pub fn cleanup_on_quit(state: &AppState) {
    let end_at = state
        .last_tick_wall
        .lock()
        .clone()
        .or_else(|| with_db(state, |db| db.get_heartbeat()).ok().flatten())
        .unwrap_or_else(now_iso);
    let _ = with_db(state, |db| {
        db.close_all_open_sessions(&end_at)?;
        if let Some(run_id) = state.current_run_id.lock().take() {
            let _ = db.end_tracking_run(&run_id, &end_at);
        }
        db.set_heartbeat(&end_at)?;
        Ok(())
    });
    *state.current_session_id.lock() = None;
    *state.current_run_id.lock() = None;
    *state.auto_paused.lock() = false;
}

fn format_reminder_when(task: &TaskInstance) -> String {
    let hour = task.reminder_hour.unwrap_or(0);
    let minute = task.reminder_minute.unwrap_or(0);
    let (h12, ampm) = if hour == 0 {
        (12, "AM")
    } else if hour < 12 {
        (hour, "AM")
    } else if hour == 12 {
        (12, "PM")
    } else {
        (hour - 12, "PM")
    };
    format!("{h12}:{minute:02} {ampm} US CT")
}

fn check_and_fire_reminders(app: &AppHandle, state: &AppState) {
    let due = match with_db(state, |db| db.due_reminders()) {
        Ok(v) => v,
        Err(_) => return,
    };
    if due.is_empty() {
        return;
    }
    for task in due {
        let when = format_reminder_when(&task);
        let _ = with_db(state, |db| db.mark_reminder_fired(&task.id));
        notify_task_reminder(&task.summary, &format!("Due · {when}"));
        alert_task_reminder(&format!("{} ({})", task.summary, when));
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

fn soft_nudge_eligibles(tasks: &[TaskInstance], work_day: &str) -> Vec<SoftNudgeEligible> {
    let now = chrono::Utc::now();
    tasks
        .iter()
        .filter(|t| t.status != "done" && !t.summary.trim().is_empty())
        .filter_map(|t| {
            if !t.reminder_enabled {
                return Some(SoftNudgeEligible {
                    task_id: t.id.clone(),
                    before: None,
                });
            }
            // Hard-alarm tasks: soft heads-up only while the alarm is still ahead.
            if t.reminder_fired {
                return None;
            }
            let hour = t.reminder_hour?;
            let minute = t.reminder_minute?;
            let at = reminder_instant_in_work_day(work_day, hour, minute, &t.reminder_tz)?;
            if at <= now {
                return None;
            }
            Some(SoftNudgeEligible {
                task_id: t.id.clone(),
                before: Some(at),
            })
        })
        .collect()
}

fn check_and_fire_soft_nudges(app: &AppHandle, state: &AppState) {
    let settings = match with_db(state, |db| db.get_settings()) {
        Ok(s) => s,
        Err(_) => return,
    };
    if !settings.soft_nudges_enabled {
        return;
    }

    let date = today_date();
    let tasks = with_db(state, |db| db.list_tasks_for_date(&date)).unwrap_or_default();
    let eligible = soft_nudge_eligibles(&tasks, &date);

    let mut plan = with_db(state, |db| db.get_soft_nudge_plan()).unwrap_or_default();
    plan = sync_soft_nudge_plan(plan, &eligible);
    let _ = with_db(state, |db| db.save_soft_nudge_plan(&plan));

    let now = chrono::Utc::now();
    let mut changed = false;
    for slot in &mut plan.slots {
        if slot.fired || slot.task_id.is_empty() {
            continue;
        }
        let Some(at) = chrono::DateTime::parse_from_rfc3339(&slot.at)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc))
        else {
            continue;
        };
        if now < at {
            continue;
        }
        slot.fired = true;
        changed = true;

        let Some(task) = tasks.iter().find(|t| t.id == slot.task_id) else {
            continue;
        };
        if task.status == "done" {
            continue;
        }
        // Alarm already fired — hard alert covers it; skip soft heads-up.
        if task.reminder_enabled && task.reminder_fired {
            continue;
        }
        let _ = with_db(state, |db| db.mark_soft_nudge_fired(&task.id));
        push_soft_nudge(app, state, task);
    }
    if changed {
        let _ = with_db(state, |db| db.save_soft_nudge_plan(&plan));
    }
}

/// Soft nudge toast — prefer the large workspace display, not the laptop tray screen.
fn show_nudge_toast(app: &AppHandle, state: &AppState, banner: &SoftNudgeBanner) {
    let app = app.clone();
    let banner = banner.clone();
    let cached = *state.last_tray_rect.lock();
    let _ = app.clone().run_on_main_thread(move || {
        position_and_show_nudge(&app, cached, &banner);
    });
}

fn resolve_tray_anchor(
    app: &AppHandle,
    cached: Option<crate::TrayRectCache>,
    scale: f64,
) -> Option<(f64, f64, f64, f64)> {
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(Some(rect)) = tray.rect() {
            let pos = rect.position.to_physical::<f64>(scale);
            let size = rect.size.to_physical::<f64>(scale);
            let cache = crate::TrayRectCache {
                x: pos.x,
                y: pos.y,
                width: size.width.max(1.0),
                height: size.height.max(1.0),
            };
            *app.state::<AppState>().last_tray_rect.lock() = Some(cache);
            return Some((cache.x, cache.y, cache.width, cache.height));
        }
    }
    cached.map(|c| (c.x, c.y, c.width.max(1.0), c.height.max(1.0)))
}

fn monitor_area(m: &tauri::Monitor) -> u64 {
    let s = m.size();
    (s.width as u64).saturating_mul(s.height as u64)
}

fn point_on_monitor(m: &tauri::Monitor, x: f64, y: f64) -> bool {
    let p = m.position();
    let s = m.size();
    let left = p.x as f64;
    let top = p.y as f64;
    let right = left + s.width as f64;
    let bottom = top + s.height as f64;
    x >= left && x < right && y >= top && y < bottom
}

/// Prefer the large workspace display over a small laptop panel.
/// Cursor wins only when it's already on a large (≈largest) screen.
fn resolve_work_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    let monitors = app.available_monitors().ok().unwrap_or_default();
    if monitors.is_empty() {
        return app.primary_monitor().ok().flatten();
    }
    if monitors.len() == 1 {
        return monitors.into_iter().next();
    }

    let largest = monitors
        .iter()
        .max_by_key(|m| monitor_area(m))
        .cloned()?;
    let largest_area = monitor_area(&largest);

    if let Ok(cursor) = app.cursor_position() {
        if let Ok(Some(under_cursor)) = app.monitor_from_point(cursor.x, cursor.y) {
            let cur_area = monitor_area(&under_cursor);
            // Ignore the tiny laptop if the cursor briefly sits there (e.g. Teams).
            if cur_area.saturating_mul(100) >= largest_area.saturating_mul(80) {
                return Some(under_cursor);
            }
        }
    }

    Some(largest)
}

fn position_and_show_nudge(
    app: &AppHandle,
    cached: Option<crate::TrayRectCache>,
    banner: &SoftNudgeBanner,
) {
    let Some(w) = app.get_webview_window("nudge") else {
        return;
    };

    let monitor = resolve_work_monitor(app).or_else(|| w.primary_monitor().ok().flatten());
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(2.0);

    let logical_w = 336.0_f64;
    let logical_h = 138.0_f64;
    let phys_w = (logical_w * scale).round().max(1.0);
    let phys_h = (logical_h * scale).round().max(1.0);

    let tray = resolve_tray_anchor(app, cached, scale);
    let tray_on_work_monitor = match (&monitor, tray) {
        (Some(m), Some((tx, ty, tw, th))) => {
            point_on_monitor(m, tx + tw / 2.0, ty + th / 2.0)
        }
        _ => false,
    };

    let (x, y) = if tray_on_work_monitor {
        if let (Some(monitor), Some((tx, ty, tw, th))) = (monitor.as_ref(), tray) {
            let center_x = tx + tw / 2.0;
            let mut x = (center_x - phys_w / 2.0).round();
            let mut y = (ty + th + 2.0 * scale).round();
            let mpos = monitor.position();
            let msize = monitor.size();
            let left = mpos.x as f64;
            let top = mpos.y as f64;
            let right = left + msize.width as f64;
            let bottom = top + msize.height as f64;
            x = x.clamp(left + 8.0, (right - phys_w - 8.0).max(left + 8.0));
            y = y.clamp(top + 2.0, (bottom - phys_h - 8.0).max(top + 2.0));
            (x as i32, y as i32)
        } else {
            (40, 40)
        }
    } else if let Some(monitor) = monitor.as_ref() {
        // Tray lives on the laptop — put the toast top-right on the big screen.
        let mpos = monitor.position();
        let msize = monitor.size();
        let x = (mpos.x as f64 + msize.width as f64 - phys_w - 16.0 * scale).round() as i32;
        let y = (mpos.y as f64 + 28.0 * scale).round() as i32;
        (x, y)
    } else {
        (40, 40)
    };

    let _ = w.set_size(Size::Physical(PhysicalSize {
        width: phys_w as u32,
        height: phys_h as u32,
    }));
    let _ = w.set_position(Position::Physical(PhysicalPosition { x, y }));
    let _ = w.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    let _ = w.set_always_on_top(true);
    let _ = w.show();
    let _ = w.emit("soft-nudge", banner.clone());
    let _ = app.emit("soft-nudge", banner.clone());
}

fn push_soft_nudge(app: &AppHandle, state: &AppState, task: &TaskInstance) {
    let settings = with_db(state, |db| db.get_settings()).ok();
    let theme = settings
        .as_ref()
        .map(|s| crate::db::normalize_app_theme(&s.app_theme))
        .unwrap_or_else(|| "legacy".into());
    let style = if theme == "liquidGlass" {
        "liquidGlass".to_string()
    } else {
        "classic".to_string()
    };
    let (glass_level, glass_reduce_transparency, glass_increase_contrast) = settings
        .map(|s| {
            (
                crate::db::clamp_glass_level(s.glass_level),
                s.glass_reduce_transparency,
                s.glass_increase_contrast,
            )
        })
        .unwrap_or((55, false, false));

    let summary = task.summary.trim();
    let (kind, alarm_time, message) = if task.reminder_enabled {
        let hour = task.reminder_hour.unwrap_or(0);
        let minute = task.reminder_minute.unwrap_or(0);
        let time_label = format_reminder_time_label(hour, minute, &task.reminder_tz);
        (
            "alarmHeadsUp".to_string(),
            Some(time_label.clone()),
            format!(
                "Just letting you know that task \"{summary}\" is set to be done at {time_label} today."
            ),
        )
    } else {
        (
            "todo".to_string(),
            None,
            format!(
                "Have you completed \"{summary}\" yet? Don't forget to do this before signing off today."
            ),
        )
    };

    let banner = SoftNudgeBanner {
        summary: summary.to_string(),
        message,
        at: now_iso(),
        kind,
        alarm_time,
        style,
        glass_level,
        glass_reduce_transparency,
        glass_increase_contrast,
    };
    *state.last_soft_nudge.lock() = Some(banner.clone());
    show_nudge_toast(app, state, &banner);
}

/// Used only when launching with `PROMO_DEMO=1` for screen recordings.
pub fn fire_promo_soft_nudge(app: &AppHandle, state: &AppState) {
    let date = today_date();
    let tasks = with_db(state, |db| db.list_tasks_for_date(&date)).unwrap_or_default();
    let task = tasks
        .iter()
        .find(|t| t.reminder_enabled && t.status != "done")
        .or_else(|| tasks.iter().find(|t| t.status != "done"));
    if let Some(task) = task {
        push_soft_nudge(app, state, task);
    }
}

/// Fire one soft nudge immediately so the user can preview the UI.
#[tauri::command]
pub fn preview_soft_nudge(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SoftNudgeBanner, String> {
    let date = today_date();
    let _ = with_db(&state, |db| db.ensure_today_tasks());
    let tasks = with_db(&state, |db| db.list_tasks_for_date(&date))?;
    let eligible = soft_nudge_eligibles(&tasks, &date);
    let Some(first) = eligible.first() else {
        return Err(
            "No pending tasks to preview. Add an incomplete todo (with or without an Alarm)."
                .into(),
        );
    };
    let Some(task) = tasks.iter().find(|t| t.id == first.task_id) else {
        return Err("Task not found for preview.".into());
    };
    push_soft_nudge(&app, &state, task);
    state
        .last_soft_nudge
        .lock()
        .clone()
        .ok_or_else(|| "Failed to create preview.".into())
}

#[tauri::command]
pub fn dismiss_soft_nudge(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    *state.last_soft_nudge.lock() = None;
    if let Some(w) = app.get_webview_window("nudge") {
        let _ = w.hide();
    }
    Ok(())
}

fn get_today_inner(state: &AppState) -> Result<TodaySnapshot, String> {
    // One tick for tray + summary; avoid double osascript.
    let current = tick_collector(state)?;
    let date = today_date();
    let tasks = with_db(state, |db| db.ensure_today_tasks()).unwrap_or_default();
    let summary = with_db(state, |db| db.day_summary(&date)).unwrap_or(DaySummary {
        date: date.clone(),
        window_start: String::new(),
        window_end: String::new(),
        active_seconds: 0,
        idle_seconds: 0,
        productive_seconds: 0,
        entertainment_seconds: 0,
        communication_seconds: 0,
        browsing_seconds: 0,
        other_seconds: 0,
        by_category: vec![],
    });
    let tasks_done = tasks.iter().filter(|t| t.status == "done").count();
    let tasks_total = tasks.len();
    let tracking_enabled = current.tracking_enabled;
    let activity_log = with_db(state, |db| db.activity_log_for_date(&date)).unwrap_or_default();
    let soft_nudge = state.last_soft_nudge.lock().clone();
    Ok(TodaySnapshot {
        date,
        clocks: dual_clocks(),
        tasks,
        summary,
        current,
        tasks_done,
        tasks_total,
        tracking_enabled,
        activity_log,
        soft_nudge,
    })
}
