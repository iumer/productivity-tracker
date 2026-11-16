use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::{America::Chicago, Asia::Karachi, Tz};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub idle_threshold_minutes: u32,
    /// length 7, index 0 = Sunday … 6 = Saturday
    pub day_enabled: [bool; 7],
    pub accessibility_prompted: bool,
    /// First-launch permissions checklist completed.
    #[serde(default)]
    pub onboarding_completed: bool,
    /// When false, activity collection is paused.
    #[serde(default = "default_tracking_enabled")]
    pub tracking_enabled: bool,
    /// Soft random nudges for incomplete tasks (10AM–6PM US CT); alarm tasks get a heads-up.
    #[serde(default = "default_soft_nudges_enabled")]
    pub soft_nudges_enabled: bool,
    /// App + soft-nudge look: `legacy` or `liquidGlass`.
    /// Accepts older `softNudgeStyle` / `classic` values from saved settings.
    #[serde(
        default = "default_app_theme",
        alias = "softNudgeStyle"
    )]
    pub app_theme: String,
    /// Liquid Glass frost amount 0–100 (0 = clear, 100 = frosted). macOS 27–style.
    #[serde(default = "default_glass_level")]
    pub glass_level: u32,
    /// Accessibility-style: push opacity up for readability.
    #[serde(default)]
    pub glass_reduce_transparency: bool,
    /// Stronger text contrast on glass surfaces.
    #[serde(default)]
    pub glass_increase_contrast: bool,
}

fn default_tracking_enabled() -> bool {
    false
}

fn default_soft_nudges_enabled() -> bool {
    true
}

fn default_app_theme() -> String {
    "legacy".into()
}

fn default_glass_level() -> u32 {
    55
}

/// Normalize persisted theme strings (`classic` → `legacy`).
pub fn normalize_app_theme(raw: &str) -> String {
    match raw {
        "liquidGlass" => "liquidGlass".into(),
        _ => "legacy".into(),
    }
}

pub fn clamp_glass_level(level: u32) -> u32 {
    level.min(100)
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_threshold_minutes: 5,
            day_enabled: [false, true, true, true, true, true, false],
            accessibility_prompted: false,
            onboarding_completed: false,
            tracking_enabled: false,
            soft_nudges_enabled: true,
            app_theme: default_app_theme(),
            glass_level: default_glass_level(),
            glass_reduce_transparency: false,
            glass_increase_contrast: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SoftNudgePlan {
    /// Chicago calendar date YYYY-MM-DD this plan belongs to.
    pub date_ct: String,
    /// One slot per eligible task, random time in 10AM–6PM CT (before hard alarm if set).
    pub slots: Vec<SoftNudgeSlot>,
}

/// Task eligible for a soft nudge today, with optional hard-alarm deadline.
#[derive(Debug, Clone)]
pub struct SoftNudgeEligible {
    pub task_id: String,
    /// Soft nudge must fire before this instant (hard alarm still ahead).
    pub before: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftNudgeSlot {
    pub at: String,
    /// Task this slot will nudge (guarantees coverage).
    #[serde(default)]
    pub task_id: String,
    pub fired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryRule {
    pub id: String,
    pub label: String,
    pub display: String,
    /// Match these app names (case-insensitive contains)
    pub match_apps: Vec<String>,
    /// If non-empty, window title must contain one of these (case-insensitive)
    #[serde(default)]
    pub title_contains: Vec<String>,
    /// If non-empty, page URL must contain one of these (case-insensitive; use domains)
    #[serde(default)]
    pub url_contains: Vec<String>,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTemplate {
    pub id: String,
    /// 0=Sun … 6=Sat
    pub weekday: i32,
    pub summary: String,
    pub sort_order: i32,
    #[serde(default)]
    pub reminder_enabled: bool,
    /// 0–23 when reminder_enabled
    #[serde(default)]
    pub reminder_hour: Option<u32>,
    /// 0–59 when reminder_enabled
    #[serde(default)]
    pub reminder_minute: Option<u32>,
    /// IANA tz: Asia/Karachi or America/Chicago
    #[serde(default = "default_reminder_tz")]
    pub reminder_tz: String,
}

fn default_reminder_tz() -> String {
    "America/Chicago".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInstance {
    pub id: String,
    pub date: String,
    pub template_id: Option<String>,
    pub summary: String,
    pub ticket_link: String,
    pub notes: String,
    pub status: String,
    pub sort_order: i32,
    #[serde(default)]
    pub reminder_enabled: bool,
    #[serde(default)]
    pub reminder_hour: Option<u32>,
    #[serde(default)]
    pub reminder_minute: Option<u32>,
    #[serde(default = "default_reminder_tz")]
    pub reminder_tz: String,
    /// True after a popup/notification was shown for this instance.
    #[serde(default)]
    pub reminder_fired: bool,
    /// Soft random nudge already shown today (tasks without hard alarms).
    #[serde(default)]
    pub soft_nudge_fired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySession {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub app_name: String,
    pub window_title: String,
    pub category_id: String,
    pub category_label: String,
    pub is_idle: bool,
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingRun {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLogEntry {
    pub id: String,
    /// "start" | "stop" | "activity"
    pub kind: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTime {
    pub category_id: String,
    pub category_label: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub date: String,
    /// Inclusive start of work window (ISO).
    pub window_start: String,
    /// Exclusive end of work window (ISO).
    pub window_end: String,
    pub active_seconds: i64,
    pub idle_seconds: i64,
    pub productive_seconds: i64,
    pub entertainment_seconds: i64,
    pub communication_seconds: i64,
    pub browsing_seconds: i64,
    pub other_seconds: i64,
    pub by_category: Vec<CategoryTime>,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS category_rules (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                display TEXT NOT NULL,
                match_apps TEXT NOT NULL,
                title_contains TEXT NOT NULL,
                priority INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS task_templates (
                id TEXT PRIMARY KEY,
                weekday INTEGER NOT NULL,
                summary TEXT NOT NULL,
                sort_order INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS task_instances (
                id TEXT PRIMARY KEY,
                date TEXT NOT NULL,
                template_id TEXT,
                summary TEXT NOT NULL,
                ticket_link TEXT NOT NULL DEFAULT '',
                notes TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'todo',
                sort_order INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS activity_sessions (
                id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                app_name TEXT NOT NULL,
                window_title TEXT NOT NULL,
                category_id TEXT NOT NULL,
                category_label TEXT NOT NULL,
                is_idle INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS tracking_runs (
                id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                ended_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_task_instances_date ON task_instances(date);
            CREATE INDEX IF NOT EXISTS idx_activity_started ON activity_sessions(started_at);
            CREATE INDEX IF NOT EXISTS idx_tracking_runs_started ON tracking_runs(started_at);
            "#,
        )?;
        // Older DBs: add run_id if missing.
        let _ = self
            .conn
            .execute("ALTER TABLE activity_sessions ADD COLUMN run_id TEXT", []);
        // Task reminder columns (alarms / popups).
        for sql in [
            "ALTER TABLE task_templates ADD COLUMN reminder_enabled INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE task_templates ADD COLUMN reminder_hour INTEGER",
            "ALTER TABLE task_templates ADD COLUMN reminder_minute INTEGER",
            "ALTER TABLE task_templates ADD COLUMN reminder_tz TEXT NOT NULL DEFAULT 'Asia/Karachi'",
            "ALTER TABLE task_instances ADD COLUMN reminder_enabled INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE task_instances ADD COLUMN reminder_hour INTEGER",
            "ALTER TABLE task_instances ADD COLUMN reminder_minute INTEGER",
            "ALTER TABLE task_instances ADD COLUMN reminder_tz TEXT NOT NULL DEFAULT 'Asia/Karachi'",
            "ALTER TABLE task_instances ADD COLUMN reminder_fired INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE category_rules ADD COLUMN url_contains TEXT NOT NULL DEFAULT '[]'",
            "ALTER TABLE task_instances ADD COLUMN soft_nudge_fired INTEGER NOT NULL DEFAULT 0",
        ] {
            let _ = self.conn.execute(sql, []);
        }
        Ok(())
    }

    pub fn get_settings(&self) -> Result<AppSettings, rusqlite::Error> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'app'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match row {
            Some(json) => {
                let mut s: AppSettings = serde_json::from_str(&json).unwrap_or_default();
                s.app_theme = normalize_app_theme(&s.app_theme);
                s.glass_level = clamp_glass_level(s.glass_level);
                Ok(s)
            }
            None => {
                let s = AppSettings::default();
                self.save_settings(&s)?;
                Ok(s)
            }
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), rusqlite::Error> {
        let mut settings = settings.clone();
        settings.app_theme = normalize_app_theme(&settings.app_theme);
        settings.glass_level = clamp_glass_level(settings.glass_level);
        let json = serde_json::to_string(&settings).unwrap();
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn get_heartbeat(&self) -> Result<Option<String>, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'heartbeat'",
                [],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn set_heartbeat(&self, iso: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES('heartbeat', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![iso],
        )?;
        Ok(())
    }

    pub fn get_soft_nudge_plan(&self) -> Result<SoftNudgePlan, rusqlite::Error> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'soft_nudge_plan'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(row
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default())
    }

    pub fn save_soft_nudge_plan(&self, plan: &SoftNudgePlan) -> Result<(), rusqlite::Error> {
        let json = serde_json::to_string(plan).unwrap();
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES('soft_nudge_plan', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    /// Close orphans at last heartbeat (not now).
    /// Sleep / lid-closed gaps are **not** filled with idle — uncounted time.
    pub fn heal_orphans_from_heartbeat(&self) -> Result<(), rusqlite::Error> {
        let Some(hb) = self.get_heartbeat()? else {
            let now = now_iso();
            let _ = self.close_all_open_sessions(&now);
            return Ok(());
        };
        let _ = self.close_all_open_sessions(&hb)?;
        Ok(())
    }

    /// After crash / force-quit: clip sessions, end stray runs, or reopen a run
    /// if the user still has tracking enabled.
    pub fn reconcile_tracking_on_startup(&self) -> Result<(), rusqlite::Error> {
        self.heal_orphans_from_heartbeat()?;
        let settings = self.get_settings()?;
        let end_at = self
            .get_heartbeat()?
            .unwrap_or_else(now_iso);
        if let Some(run) = self.get_open_tracking_run()? {
            if !settings.tracking_enabled {
                // Preference is off — close the runaway run at last heartbeat.
                self.end_tracking_run(&run.id, &end_at)?;
            }
            // Preference still on: keep the open run so time continues under one boundary.
        } else if settings.tracking_enabled {
            // Was tracking but run row missing after a crash — start a clean run now.
            let _ = self.start_tracking_run(&now_iso())?;
        }
        Ok(())
    }

    pub fn count_category_rules(&self) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM category_rules", [], |r| r.get(0))
    }

    pub fn has_category_rule(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM category_rules WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn count_templates(&self) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM task_templates", [], |r| r.get(0))
    }

    pub fn has_any_activity(&self) -> Result<bool, rusqlite::Error> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM activity_sessions LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn list_category_rules(&self) -> Result<Vec<CategoryRule>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, display, match_apps, title_contains, priority, enabled, url_contains
             FROM category_rules ORDER BY priority DESC, label ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            let match_apps: String = r.get(3)?;
            let title_contains: String = r.get(4)?;
            let url_contains: String = r.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".into());
            Ok(CategoryRule {
                id: r.get(0)?,
                label: r.get(1)?,
                display: r.get(2)?,
                match_apps: serde_json::from_str(&match_apps).unwrap_or_default(),
                title_contains: serde_json::from_str(&title_contains).unwrap_or_default(),
                url_contains: serde_json::from_str(&url_contains).unwrap_or_default(),
                priority: r.get(5)?,
                enabled: r.get::<_, i32>(6)? == 1,
            })
        })?;
        rows.collect()
    }

    pub fn save_category_rules(&self, rules: &[CategoryRule]) -> Result<(), rusqlite::Error> {
        self.conn.execute("DELETE FROM category_rules", [])?;
        for rule in rules {
            self.insert_category_rule(rule)?;
        }
        Ok(())
    }

    pub fn insert_category_rule(&self, rule: &CategoryRule) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO category_rules(id, label, display, match_apps, title_contains, priority, enabled, url_contains)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                rule.id,
                rule.label,
                rule.display,
                serde_json::to_string(&rule.match_apps).unwrap(),
                serde_json::to_string(&rule.title_contains).unwrap(),
                rule.priority,
                if rule.enabled { 1 } else { 0 },
                serde_json::to_string(&rule.url_contains).unwrap(),
            ],
        )?;
        Ok(())
    }

    /// Persist an auto-discovered app rule once (keeps user enabled/disabled later).
    pub fn ensure_auto_app_rule(&self, app_name: &str) -> Result<(), rusqlite::Error> {
        if crate::classifier::is_noise_app(app_name) {
            return Ok(());
        }
        let rule = crate::classifier::auto_app_rule(app_name);
        if self.has_category_rule(&rule.id)? {
            return Ok(());
        }
        self.insert_category_rule(&rule)
    }

    /// Move past "Other" rows that already have an app name into named app
    /// categories so dashboard bars keep their accumulated time. Leaves noise
    /// helpers and empty app_name as Other.
    pub fn reclassify_named_other_sessions(&self) -> Result<usize, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT app_name FROM activity_sessions
             WHERE category_id = 'other'
               AND trim(app_name) != ''
             ORDER BY app_name ASC",
        )?;
        let apps: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut updated = 0usize;
        for app_name in apps {
            if crate::classifier::is_noise_app(&app_name) {
                continue;
            }
            self.ensure_auto_app_rule(&app_name)?;
            let rule = crate::classifier::auto_app_rule(&app_name);
            let n = self.conn.execute(
                "UPDATE activity_sessions
                 SET category_id = ?1, category_label = ?2
                 WHERE category_id = 'other' AND app_name = ?3",
                params![rule.id, rule.label, app_name],
            )?;
            updated += n;
        }
        Ok(updated)
    }

    /// Fold noise/helper apps that were auto-named back into Other.
    pub fn fold_noise_apps_into_other(&self) -> Result<usize, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT app_name FROM activity_sessions
             WHERE category_id LIKE 'app-%'
               AND trim(app_name) != ''",
        )?;
        let apps: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut updated = 0usize;
        for app_name in apps {
            if !crate::classifier::is_noise_app(&app_name) {
                continue;
            }
            let rule_id = crate::classifier::auto_app_id(&app_name);
            let n = self.conn.execute(
                "UPDATE activity_sessions
                 SET category_id = 'other', category_label = 'Other'
                 WHERE app_name = ?1 AND category_id = ?2",
                params![app_name, rule_id],
            )?;
            updated += n;
            let _ = self
                .conn
                .execute("DELETE FROM category_rules WHERE id = ?1", params![rule_id]);
        }
        Ok(updated)
    }

    /// Merge seed title/url patterns / display into an existing rule (keeps enabled flag).
    pub fn upsert_category_rule_meta(&self, rule: &CategoryRule) -> Result<(), rusqlite::Error> {
        if !self.has_category_rule(&rule.id)? {
            return self.insert_category_rule(rule);
        }
        self.conn.execute(
            "UPDATE category_rules SET
                label = ?1,
                display = ?2,
                match_apps = ?3,
                title_contains = ?4,
                priority = ?5,
                url_contains = ?6
             WHERE id = ?7",
            params![
                rule.label,
                rule.display,
                serde_json::to_string(&rule.match_apps).unwrap(),
                serde_json::to_string(&rule.title_contains).unwrap(),
                rule.priority,
                serde_json::to_string(&rule.url_contains).unwrap(),
                rule.id,
            ],
        )?;
        Ok(())
    }

    pub fn list_templates(&self) -> Result<Vec<TaskTemplate>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, weekday, summary, sort_order,
                    reminder_enabled, reminder_hour, reminder_minute, reminder_tz
             FROM task_templates
             ORDER BY weekday ASC, sort_order ASC",
        )?;
        let rows = stmt.query_map([], row_to_template)?;
        rows.collect()
    }

    pub fn list_templates_for_weekday(
        &self,
        weekday: i32,
    ) -> Result<Vec<TaskTemplate>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, weekday, summary, sort_order,
                    reminder_enabled, reminder_hour, reminder_minute, reminder_tz
             FROM task_templates
             WHERE weekday = ?1 ORDER BY sort_order ASC",
        )?;
        let rows = stmt.query_map(params![weekday], row_to_template)?;
        rows.collect()
    }

    pub fn insert_template(&self, t: &TaskTemplate) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO task_templates(
                id, weekday, summary, sort_order,
                reminder_enabled, reminder_hour, reminder_minute, reminder_tz
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                t.id,
                t.weekday,
                t.summary,
                t.sort_order,
                if t.reminder_enabled { 1 } else { 0 },
                t.reminder_hour.map(|h| h as i64),
                t.reminder_minute.map(|m| m as i64),
                normalize_tz(&t.reminder_tz),
            ],
        )?;
        Ok(())
    }

    pub fn replace_templates_for_weekday(
        &self,
        weekday: i32,
        templates: &[TaskTemplate],
    ) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM task_templates WHERE weekday = ?1", params![weekday])?;
        for t in templates {
            self.insert_template(t)?;
        }
        // If this weekday is the current work day, refresh Today's checklist too.
        self.sync_today_from_templates_if_current_weekday(weekday)?;
        Ok(())
    }

    /// When saving templates for today's work-day weekday, rewrite today's
    /// template-sourced tasks to match. Keeps ad-hoc tasks and preserves
    /// done/notes/links when summaries still match.
    pub fn sync_today_from_templates_if_current_weekday(
        &self,
        weekday: i32,
    ) -> Result<(), rusqlite::Error> {
        if today_weekday() as i32 != weekday {
            return Ok(());
        }
        let today = today_date();
        let templates = self.list_templates_for_weekday(weekday)?;
        let existing = self.list_tasks_for_date(&today)?;

        let adhoc: Vec<TaskInstance> = existing
            .iter()
            .filter(|t| t.template_id.is_none())
            .cloned()
            .collect();
        let templated: Vec<TaskInstance> = existing
            .iter()
            .filter(|t| t.template_id.is_some())
            .cloned()
            .collect();

        for old in &templated {
            self.delete_task(&old.id)?;
        }

        for (i, t) in templates.iter().enumerate() {
            let prior = find_matching_task(&templated, &t.summary)
                .or_else(|| find_matching_task(&existing, &t.summary));
            let reminder_same = prior
                .map(|p| {
                    p.reminder_enabled == t.reminder_enabled
                        && p.reminder_hour == t.reminder_hour
                        && p.reminder_minute == t.reminder_minute
                })
                .unwrap_or(false);
            let instance = TaskInstance {
                id: Uuid::new_v4().to_string(),
                date: today.clone(),
                template_id: Some(t.id.clone()),
                summary: t.summary.clone(),
                ticket_link: prior.map(|p| p.ticket_link.clone()).unwrap_or_default(),
                notes: prior.map(|p| p.notes.clone()).unwrap_or_default(),
                status: prior
                    .map(|p| p.status.clone())
                    .unwrap_or_else(|| "todo".into()),
                sort_order: i as i32,
                reminder_enabled: t.reminder_enabled,
                reminder_hour: t.reminder_hour,
                reminder_minute: t.reminder_minute,
                reminder_tz: t.reminder_tz.clone(),
                // Keep fired only if the alarm time didn't change.
                reminder_fired: reminder_same
                    && prior.map(|p| p.reminder_fired).unwrap_or(false),
                soft_nudge_fired: prior.map(|p| p.soft_nudge_fired).unwrap_or(false),
            };
            self.insert_task(&instance)?;
        }

        let base = templates.len() as i32;
        for (i, mut task) in adhoc.into_iter().enumerate() {
            task.sort_order = base + i as i32;
            self.update_task(&task)?;
        }
        Ok(())
    }

    pub fn delete_template(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM task_templates WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_tasks_for_date(&self, date: &str) -> Result<Vec<TaskInstance>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, date, template_id, summary, ticket_link, notes, status, sort_order,
                    reminder_enabled, reminder_hour, reminder_minute, reminder_tz, reminder_fired,
                    soft_nudge_fired
             FROM task_instances WHERE date = ?1 ORDER BY sort_order ASC",
        )?;
        let rows = stmt.query_map(params![date], row_to_task)?;
        rows.collect()
    }

    pub fn ensure_today_tasks(&self) -> Result<Vec<TaskInstance>, rusqlite::Error> {
        // Work day is Pakistan 5PM→5PM window.
        let today = today_date();
        let existing = self.list_tasks_for_date(&today)?;
        if !existing.is_empty() {
            // Persist any Today-only adds into weekday templates for future days.
            let _ = self.mirror_todays_adhoc_tasks_to_templates();
            return Ok(self.list_tasks_for_date(&today)?);
        }

        let settings = self.get_settings()?;
        let weekday = today_weekday();
        if !settings.day_enabled.get(weekday).copied().unwrap_or(false) {
            return Ok(vec![]);
        }

        let templates = self.list_templates_for_weekday(weekday as i32)?;
        for (i, t) in templates.iter().enumerate() {
            let instance = TaskInstance {
                id: Uuid::new_v4().to_string(),
                date: today.clone(),
                template_id: Some(t.id.clone()),
                summary: t.summary.clone(),
                ticket_link: String::new(),
                notes: String::new(),
                status: "todo".into(),
                sort_order: i as i32,
                reminder_enabled: t.reminder_enabled,
                reminder_hour: t.reminder_hour,
                reminder_minute: t.reminder_minute,
                reminder_tz: t.reminder_tz.clone(),
                reminder_fired: false,
                soft_nudge_fired: false,
            };
            self.insert_task(&instance)?;
        }
        self.list_tasks_for_date(&today)
    }

    pub fn update_template_reminder_and_summary(
        &self,
        id: &str,
        summary: &str,
        reminder_enabled: bool,
        reminder_hour: Option<u32>,
        reminder_minute: Option<u32>,
        reminder_tz: &str,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE task_templates SET
                summary = ?1,
                reminder_enabled = ?2,
                reminder_hour = ?3,
                reminder_minute = ?4,
                reminder_tz = ?5
             WHERE id = ?6",
            params![
                summary,
                if reminder_enabled { 1 } else { 0 },
                reminder_hour.map(|h| h as i64),
                reminder_minute.map(|m| m as i64),
                normalize_tz(reminder_tz),
                id,
            ],
        )?;
        Ok(())
    }

    /// Keep the weekday template in sync when Today's checklist is edited.
    pub fn sync_template_from_task(&self, task: &TaskInstance) -> Result<(), rusqlite::Error> {
        let Some(ref tid) = task.template_id else {
            return Ok(());
        };
        self.update_template_reminder_and_summary(
            tid,
            &task.summary,
            task.reminder_enabled,
            task.reminder_hour,
            task.reminder_minute,
            &task.reminder_tz,
        )
    }

    /// Create (or reuse) a weekday template for an ad-hoc Today task and link it.
    pub fn mirror_task_to_weekday_template(
        &self,
        task: &mut TaskInstance,
    ) -> Result<(), rusqlite::Error> {
        if task.template_id.is_some() {
            self.sync_template_from_task(task)?;
            return Ok(());
        }
        if task.date != today_date() {
            return Ok(());
        }
        let weekday = today_weekday() as i32;
        let templates = self.list_templates_for_weekday(weekday)?;
        if let Some(existing) = templates
            .iter()
            .find(|t| summaries_loosely_match(&t.summary, &task.summary))
        {
            task.template_id = Some(existing.id.clone());
            self.conn.execute(
                "UPDATE task_instances SET template_id = ?1 WHERE id = ?2",
                params![existing.id, task.id],
            )?;
            self.sync_template_from_task(task)?;
            return Ok(());
        }
        let tmpl = TaskTemplate {
            id: Uuid::new_v4().to_string(),
            weekday,
            summary: task.summary.trim().to_string(),
            sort_order: templates.len() as i32,
            reminder_enabled: task.reminder_enabled,
            reminder_hour: task.reminder_hour,
            reminder_minute: task.reminder_minute,
            reminder_tz: normalize_tz(&task.reminder_tz),
        };
        self.insert_template(&tmpl)?;
        task.template_id = Some(tmpl.id.clone());
        self.conn.execute(
            "UPDATE task_instances SET template_id = ?1 WHERE id = ?2",
            params![tmpl.id, task.id],
        )?;
        Ok(())
    }

    /// Backfill: ad-hoc tasks on today get weekday templates so they recur.
    pub fn mirror_todays_adhoc_tasks_to_templates(&self) -> Result<(), rusqlite::Error> {
        let today = today_date();
        let tasks = self.list_tasks_for_date(&today)?;
        for mut task in tasks {
            if task.template_id.is_some() || task.summary.trim().is_empty() {
                continue;
            }
            self.mirror_task_to_weekday_template(&mut task)?;
        }
        Ok(())
    }

    pub fn insert_task(&self, task: &TaskInstance) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO task_instances(
                id, date, template_id, summary, ticket_link, notes, status, sort_order,
                reminder_enabled, reminder_hour, reminder_minute, reminder_tz, reminder_fired,
                soft_nudge_fired
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                task.id,
                task.date,
                task.template_id,
                task.summary,
                task.ticket_link,
                task.notes,
                task.status,
                task.sort_order,
                if task.reminder_enabled { 1 } else { 0 },
                task.reminder_hour.map(|h| h as i64),
                task.reminder_minute.map(|m| m as i64),
                normalize_tz(&task.reminder_tz),
                if task.reminder_fired { 1 } else { 0 },
                if task.soft_nudge_fired { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn update_task(&self, task: &TaskInstance) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE task_instances SET
                summary=?1, ticket_link=?2, notes=?3, status=?4, sort_order=?5,
                reminder_enabled=?6, reminder_hour=?7, reminder_minute=?8,
                reminder_tz=?9, reminder_fired=?10, soft_nudge_fired=?11
             WHERE id=?12",
            params![
                task.summary,
                task.ticket_link,
                task.notes,
                task.status,
                task.sort_order,
                if task.reminder_enabled { 1 } else { 0 },
                task.reminder_hour.map(|h| h as i64),
                task.reminder_minute.map(|m| m as i64),
                normalize_tz(&task.reminder_tz),
                if task.reminder_fired { 1 } else { 0 },
                if task.soft_nudge_fired { 1 } else { 0 },
                task.id,
            ],
        )?;
        Ok(())
    }

    pub fn mark_reminder_fired(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE task_instances SET reminder_fired = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn mark_soft_nudge_fired(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE task_instances SET soft_nudge_fired = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Due reminders for today's work day that have not fired yet.
    pub fn due_reminders(&self) -> Result<Vec<TaskInstance>, rusqlite::Error> {
        let today = today_date();
        let tasks = self.list_tasks_for_date(&today)?;
        let now = Utc::now();
        let mut due = Vec::new();
        for task in tasks {
            if !task.reminder_enabled || task.reminder_fired || task.status == "done" {
                continue;
            }
            let Some(hour) = task.reminder_hour else {
                continue;
            };
            let Some(minute) = task.reminder_minute else {
                continue;
            };
            let Some(at) = reminder_instant_in_work_day(&today, hour, minute, &task.reminder_tz)
            else {
                continue;
            };
            if now >= at {
                due.push(task);
            }
        }
        Ok(due)
    }

    /// One-time-safe: only migrate Pakistan-tz alarms to US Central.
    /// Does **not** overwrite alarm times the user customized.
    pub fn patch_known_reminders(&self) -> Result<(), rusqlite::Error> {
        self.migrate_reminder_tz_to_us_ct()?;
        Ok(())
    }

    fn migrate_reminder_tz_to_us_ct(&self) -> Result<(), rusqlite::Error> {
        // Templates
        {
            let mut stmt = self.conn.prepare(
                "SELECT id, reminder_hour, reminder_minute, reminder_tz FROM task_templates
                 WHERE reminder_enabled = 1 AND reminder_hour IS NOT NULL AND reminder_minute IS NOT NULL",
            )?;
            let rows: Vec<(String, i64, i64, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .filter_map(|r| r.ok())
                .collect();
            for (id, hour, minute, tz) in rows {
                if normalize_tz(&tz) == "America/Chicago" {
                    continue;
                }
                let (h, m) = pk_hm_to_ct(hour as u32, minute as u32);
                self.conn.execute(
                    "UPDATE task_templates SET reminder_hour=?1, reminder_minute=?2, reminder_tz='America/Chicago'
                     WHERE id=?3",
                    params![h as i64, m as i64, id],
                )?;
            }
        }
        // Instances (not yet fired)
        {
            let mut stmt = self.conn.prepare(
                "SELECT id, reminder_hour, reminder_minute, reminder_tz FROM task_instances
                 WHERE reminder_enabled = 1 AND reminder_fired = 0
                   AND reminder_hour IS NOT NULL AND reminder_minute IS NOT NULL",
            )?;
            let rows: Vec<(String, i64, i64, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .filter_map(|r| r.ok())
                .collect();
            for (id, hour, minute, tz) in rows {
                if normalize_tz(&tz) == "America/Chicago" {
                    continue;
                }
                let (h, m) = pk_hm_to_ct(hour as u32, minute as u32);
                self.conn.execute(
                    "UPDATE task_instances SET reminder_hour=?1, reminder_minute=?2, reminder_tz='America/Chicago'
                     WHERE id=?3",
                    params![h as i64, m as i64, id],
                )?;
            }
        }
        Ok(())
    }

    pub fn delete_task(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM task_instances WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn open_session(&self, session: &ActivitySession) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO activity_sessions(id, started_at, ended_at, app_name, window_title, category_id, category_label, is_idle, run_id)
             VALUES(?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                session.id,
                session.started_at,
                session.app_name,
                session.window_title,
                session.category_id,
                session.category_label,
                if session.is_idle { 1 } else { 0 },
                session.run_id,
            ],
        )?;
        Ok(())
    }

    /// Insert a session that is already closed (e.g. sleep/idle gap after lid close).
    pub fn insert_closed_session(&self, session: &ActivitySession) -> Result<(), rusqlite::Error> {
        let ended = session
            .ended_at
            .clone()
            .unwrap_or_else(|| session.started_at.clone());
        self.conn.execute(
            "INSERT INTO activity_sessions(id, started_at, ended_at, app_name, window_title, category_id, category_label, is_idle, run_id)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session.id,
                session.started_at,
                ended,
                session.app_name,
                session.window_title,
                session.category_id,
                session.category_label,
                if session.is_idle { 1 } else { 0 },
                session.run_id,
            ],
        )?;
        Ok(())
    }

    pub fn close_session(&self, id: &str, ended_at: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE activity_sessions SET ended_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
            params![ended_at, id],
        )?;
        Ok(())
    }

    /// Close every open session — prevents orphaned rows from double-counting time.
    pub fn close_all_open_sessions(&self, ended_at: &str) -> Result<usize, rusqlite::Error> {
        let n = self.conn.execute(
            "UPDATE activity_sessions SET ended_at = ?1 WHERE ended_at IS NULL",
            params![ended_at],
        )?;
        Ok(n)
    }

    pub fn get_open_session(&self) -> Result<Option<ActivitySession>, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT id, started_at, ended_at, app_name, window_title, category_id, category_label, is_idle, run_id
                 FROM activity_sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
                [],
                |r| {
                    Ok(ActivitySession {
                        id: r.get(0)?,
                        started_at: r.get(1)?,
                        ended_at: r.get(2)?,
                        app_name: r.get(3)?,
                        window_title: r.get(4)?,
                        category_id: r.get(5)?,
                        category_label: r.get(6)?,
                        is_idle: r.get::<_, i32>(7)? == 1,
                        run_id: r.get(8)?,
                    })
                },
            )
            .optional()
    }

    pub fn start_tracking_run(&self, started_at: &str) -> Result<TrackingRun, rusqlite::Error> {
        // Close any runaway open run first.
        let _ = self.conn.execute(
            "UPDATE tracking_runs SET ended_at = ?1 WHERE ended_at IS NULL",
            params![started_at],
        );
        let run = TrackingRun {
            id: Uuid::new_v4().to_string(),
            started_at: started_at.to_string(),
            ended_at: None,
        };
        self.conn.execute(
            "INSERT INTO tracking_runs(id, started_at, ended_at) VALUES(?1, ?2, NULL)",
            params![run.id, run.started_at],
        )?;
        Ok(run)
    }

    pub fn end_tracking_run(&self, run_id: &str, ended_at: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE tracking_runs SET ended_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
            params![ended_at, run_id],
        )?;
        Ok(())
    }

    pub fn get_open_tracking_run(&self) -> Result<Option<TrackingRun>, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT id, started_at, ended_at FROM tracking_runs
                 WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
                [],
                |r| {
                    Ok(TrackingRun {
                        id: r.get(0)?,
                        started_at: r.get(1)?,
                        ended_at: r.get(2)?,
                    })
                },
            )
            .optional()
    }

    pub fn activity_log_for_date(&self, date: &str) -> Result<Vec<ActivityLogEntry>, rusqlite::Error> {
        let Some((day_start, day_end)) = work_day_bounds_for_id(date) else {
            return Ok(vec![]);
        };
        let now = Utc::now().with_timezone(&Karachi);
        let mut entries: Vec<ActivityLogEntry> = Vec::new();

        {
            let mut stmt = self.conn.prepare(
                "SELECT id, started_at, ended_at FROM tracking_runs",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(TrackingRun {
                    id: r.get(0)?,
                    started_at: r.get(1)?,
                    ended_at: r.get(2)?,
                })
            })?;
            for run in rows.flatten() {
                if let Some(start) = parse_instant(&run.started_at) {
                    if start >= day_start && start < day_end {
                        entries.push(ActivityLogEntry {
                            id: format!("{}-start", run.id),
                            kind: "start".into(),
                            started_at: run.started_at.clone(),
                            ended_at: None,
                            label: "Started tracking".into(),
                            detail: String::new(),
                        });
                    }
                }
                if let Some(end_s) = &run.ended_at {
                    if let Some(end) = parse_instant(end_s) {
                        if end >= day_start && end < day_end {
                            entries.push(ActivityLogEntry {
                                id: format!("{}-stop", run.id),
                                kind: "stop".into(),
                                started_at: end_s.clone(),
                                ended_at: None,
                                label: "Stopped tracking".into(),
                                detail: String::new(),
                            });
                        }
                    }
                }
            }
        }

        {
            // Only sessions recorded during a Start→Stop run (run_id set).
            let mut stmt = self.conn.prepare(
                "SELECT id, started_at, ended_at, app_name, window_title, category_id, category_label, is_idle, run_id
                 FROM activity_sessions
                 WHERE run_id IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(ActivitySession {
                    id: r.get(0)?,
                    started_at: r.get(1)?,
                    ended_at: r.get(2)?,
                    app_name: r.get(3)?,
                    window_title: r.get(4)?,
                    category_id: r.get(5)?,
                    category_label: r.get(6)?,
                    is_idle: r.get::<_, i32>(7)? == 1,
                    run_id: r.get(8)?,
                })
            })?;
            for session in rows.flatten() {
                let Some(start) = parse_instant(&session.started_at) else {
                    continue;
                };
                let end = session
                    .ended_at
                    .as_ref()
                    .and_then(|e| parse_instant(e))
                    .unwrap_or(now);
                if end <= day_start || start >= day_end {
                    continue;
                }
                let label = if session.is_idle {
                    "Idle".to_string()
                } else if !session.category_label.is_empty() {
                    session.category_label.clone()
                } else {
                    session.app_name.clone()
                };
                let detail = if session.window_title.is_empty() {
                    session.app_name.clone()
                } else {
                    format!("{} · {}", session.app_name, session.window_title)
                };
                entries.push(ActivityLogEntry {
                    id: session.id,
                    kind: "activity".into(),
                    started_at: session.started_at,
                    ended_at: session.ended_at,
                    label,
                    detail,
                });
            }
        }

        entries.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        Ok(entries)
    }

    pub fn day_summary(&self, date: &str) -> Result<DaySummary, rusqlite::Error> {
        // Work day = Pakistan 5:00 PM → next day 5:00 PM.
        // Only count sessions created while tracking was Started (run_id present).
        let Some((day_start, day_end)) = work_day_bounds_for_id(date) else {
            return Ok(empty_day_summary(date));
        };
        let now = Utc::now().with_timezone(&Karachi);

        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, app_name, window_title, category_id, category_label, is_idle, run_id
             FROM activity_sessions
             WHERE run_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ActivitySession {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                app_name: r.get(3)?,
                window_title: r.get(4)?,
                category_id: r.get(5)?,
                category_label: r.get(6)?,
                is_idle: r.get::<_, i32>(7)? == 1,
                run_id: r.get(8)?,
            })
        })?;

        let mut active = 0i64;
        let mut idle = 0i64;
        let mut productive = 0i64;
        let mut entertainment = 0i64;
        let mut communication = 0i64;
        let mut browsing = 0i64;
        let mut other = 0i64;
        let mut map: std::collections::BTreeMap<String, (String, i64)> =
            std::collections::BTreeMap::new();

        for row in rows.flatten() {
            let Some(start) = parse_instant(&row.started_at) else {
                continue;
            };
            let end = row
                .ended_at
                .as_ref()
                .and_then(|e| parse_instant(e))
                .unwrap_or(now);

            let clipped_start = start.max(day_start);
            let clipped_end = end.min(day_end);
            if clipped_end <= clipped_start {
                continue;
            }
            let secs = (clipped_end - clipped_start).num_seconds().max(0);
            if row.is_idle || row.category_id == "idle" || row.category_id == "stopped" {
                idle += secs;
            } else {
                active += secs;
                let entry = map
                    .entry(row.category_id.clone())
                    .or_insert((row.category_label.clone(), 0));
                entry.1 += secs;
                match category_bucket(&row.category_id) {
                    "entertainment" => entertainment += secs,
                    "communication" => communication += secs,
                    "browsing" => browsing += secs,
                    "productive" => productive += secs,
                    _ => other += secs,
                }
            }
        }

        let by_category = map
            .into_iter()
            .map(|(category_id, (category_label, seconds))| CategoryTime {
                category_id,
                category_label,
                seconds,
            })
            .collect();

        Ok(DaySummary {
            date: date.to_string(),
            window_start: day_start.to_rfc3339(),
            window_end: day_end.to_rfc3339(),
            active_seconds: active,
            idle_seconds: idle,
            productive_seconds: productive,
            entertainment_seconds: entertainment,
            communication_seconds: communication,
            browsing_seconds: browsing,
            other_seconds: other,
            by_category,
        })
    }
}

fn empty_day_summary(date: &str) -> DaySummary {
    let (ws, we) = work_day_bounds_for_id(date)
        .map(|(a, b)| (a.to_rfc3339(), b.to_rfc3339()))
        .unwrap_or_default();
    DaySummary {
        date: date.to_string(),
        window_start: ws,
        window_end: we,
        active_seconds: 0,
        idle_seconds: 0,
        productive_seconds: 0,
        entertainment_seconds: 0,
        communication_seconds: 0,
        browsing_seconds: 0,
        other_seconds: 0,
        by_category: vec![],
    }
}

fn category_bucket(category_id: &str) -> &'static str {
    match category_id {
        "netflix" | "youtube" => "entertainment",
        "whatsapp" | "iphone-mirroring" => "communication",
        // Generic browser time counts as productive (Zendesk/etc. already do).
        "browser-other" => "productive",
        "zendesk"
        | "support-site"
        | "knowledge-base"
        | "google-docs"
        | "company-site"
        | "company-wp-admin"
        | "cursor"
        | "windows-app"
        | "anydesk"
        | "parsec"
        | "notes"
        | "chatgpt"
        | "calendar"
        | "calculator" => "productive",
        "idle" | "stopped" => "idle",
        // True unknown / could not read the frontmost app (≠ idle).
        "other" | "unknown" => "other",
        // Auto-discovered apps (Teams, Excel, …).
        id if id.starts_with("app-")
            && (id.contains("teams")
                || id.contains("slack")
                || id.contains("discord")
                || id.contains("messages")
                || id.contains("zoom")) =>
        {
            "communication"
        }
        id if id.starts_with("app-")
            && (id.contains("spotify")
                || id.contains("music")
                || id.contains("tv")
                || id.contains("netflix")
                || id.contains("youtube")) =>
        {
            "entertainment"
        }
        id if id.starts_with("app-") => "productive",
        _ => "other",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneClock {
    pub label: String,
    pub date: String,
    pub time: String,
    pub weekday: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DualClocks {
    pub pakistan: ZoneClock,
    pub us_central: ZoneClock,
    /// Human label for the current Pakistan 5PM→5PM work window.
    pub work_day_label: String,
    pub work_day_weekday: u8,
}

fn zone_clock(label: &str, tz: Tz) -> ZoneClock {
    let now = Utc::now().with_timezone(&tz);
    ZoneClock {
        label: label.to_string(),
        date: now.format("%Y-%m-%d").to_string(),
        time: now.format("%I:%M %p").to_string(),
        weekday: now.format("%A").to_string(),
    }
}

pub fn dual_clocks() -> DualClocks {
    let start = work_day_start_pk();
    let end = start + chrono::Duration::days(1);
    let pk_now = Utc::now().with_timezone(&Karachi);
    let checklist = start.format("%A").to_string();
    DualClocks {
        pakistan: zone_clock("Pakistan", Karachi),
        us_central: zone_clock("US Central", Chicago),
        work_day_label: format!(
            "Checklist = {checklist} · {start} → {end} PK  (Pakistan clock now {pk})",
            start = start.format("%a %-I:%M %p"),
            end = end.format("%a %-I:%M %p"),
            pk = pk_now.format("%A"),
        ),
        work_day_weekday: start.weekday().num_days_from_sunday() as u8,
    }
}

fn parse_instant(value: &str) -> Option<DateTime<Tz>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.with_timezone(&Karachi))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .or_else(|| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
                .and_then(|ndt| Local.from_local_datetime(&ndt).single())
                .map(|d| d.with_timezone(&Karachi))
        })
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Start of current work day: most recent Pakistan 5:00 PM.
pub fn work_day_start_pk() -> DateTime<Tz> {
    let now = Utc::now().with_timezone(&Karachi);
    let today_5pm = Karachi
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 17, 0, 0)
        .single()
        .expect("valid karachi 5pm");
    if now >= today_5pm {
        today_5pm
    } else {
        today_5pm - chrono::Duration::days(1)
    }
}

/// Work-day id = calendar date of the Pakistan 5PM that started this window.
pub fn today_date() -> String {
    work_day_start_pk().format("%Y-%m-%d").to_string()
}

/// Weekday for task templates, based on work-day start (0=Sun … 6=Sat).
pub fn today_weekday() -> usize {
    work_day_start_pk().weekday().num_days_from_sunday() as usize
}

/// Bounds for a work-day id (date of the starting 5PM PK).
pub fn work_day_bounds_for_id(date: &str) -> Option<(DateTime<Tz>, DateTime<Tz>)> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let start = Karachi
        .with_ymd_and_hms(d.year(), d.month(), d.day(), 17, 0, 0)
        .single()?;
    let end = start + chrono::Duration::days(1);
    Some((start, end))
}

fn normalize_tz(tz: &str) -> String {
    match tz {
        "Asia/Karachi" | "Pakistan" | "PK" | "pk" => "Asia/Karachi".into(),
        _ => "America/Chicago".into(),
    }
}

fn parse_tz(tz: &str) -> Tz {
    if normalize_tz(tz) == "Asia/Karachi" {
        Karachi
    } else {
        Chicago
    }
}

fn pk_hm_to_ct(hour: u32, minute: u32) -> (u32, u32) {
    let now = Utc::now().with_timezone(&Karachi);
    let local = Karachi
        .with_ymd_and_hms(now.year(), now.month(), now.day(), hour.min(23), minute.min(59), 0)
        .single()
        .unwrap_or(now);
    let ct = local.with_timezone(&Chicago);
    (ct.hour(), ct.minute())
}

/// First HH:MM in `tz` that falls inside the Pakistan 5PM→5PM work window.
pub fn reminder_instant_in_work_day(
    work_day_id: &str,
    hour: u32,
    minute: u32,
    tz_name: &str,
) -> Option<chrono::DateTime<Utc>> {
    if hour > 23 || minute > 59 {
        return None;
    }
    let (window_start, window_end) = work_day_bounds_for_id(work_day_id)?;
    let tz = parse_tz(tz_name);
    let start_local = window_start.with_timezone(&tz);
    let mut day = start_local.date_naive();
    for _ in 0..3 {
        if let Some(candidate) = tz
            .with_ymd_and_hms(day.year(), day.month(), day.day(), hour, minute, 0)
            .single()
        {
            if candidate >= window_start && candidate < window_end {
                return Some(candidate.with_timezone(&Utc));
            }
            if candidate >= window_end {
                return None;
            }
        }
        day = day.succ_opt()?;
    }
    None
}

fn row_to_template(r: &rusqlite::Row<'_>) -> Result<TaskTemplate, rusqlite::Error> {
    let hour: Option<i64> = r.get(5)?;
    let minute: Option<i64> = r.get(6)?;
    let tz: String = r.get::<_, Option<String>>(7)?.unwrap_or_else(default_reminder_tz);
    Ok(TaskTemplate {
        id: r.get(0)?,
        weekday: r.get(1)?,
        summary: r.get(2)?,
        sort_order: r.get(3)?,
        reminder_enabled: r.get::<_, i32>(4).unwrap_or(0) == 1,
        reminder_hour: hour.map(|h| h as u32),
        reminder_minute: minute.map(|m| m as u32),
        reminder_tz: normalize_tz(&tz),
    })
}

fn summaries_loosely_match(a: &str, b: &str) -> bool {
    let a = a.trim().to_lowercase();
    let b = b.trim().to_lowercase();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    // "Check server" ↔ "Check server status"
    a.starts_with(&b) || b.starts_with(&a)
}

fn find_matching_task<'a>(tasks: &'a [TaskInstance], summary: &str) -> Option<&'a TaskInstance> {
    tasks
        .iter()
        .find(|t| summaries_loosely_match(&t.summary, summary))
}

fn row_to_task(r: &rusqlite::Row<'_>) -> Result<TaskInstance, rusqlite::Error> {
    let hour: Option<i64> = r.get(9)?;
    let minute: Option<i64> = r.get(10)?;
    let tz: String = r.get::<_, Option<String>>(11)?.unwrap_or_else(default_reminder_tz);
    Ok(TaskInstance {
        id: r.get(0)?,
        date: r.get(1)?,
        template_id: r.get(2)?,
        summary: r.get(3)?,
        ticket_link: r.get(4)?,
        notes: r.get(5)?,
        status: r.get(6)?,
        sort_order: r.get(7)?,
        reminder_enabled: r.get::<_, i32>(8).unwrap_or(0) == 1,
        reminder_hour: hour.map(|h| h as u32),
        reminder_minute: minute.map(|m| m as u32),
        reminder_tz: normalize_tz(&tz),
        reminder_fired: r.get::<_, i32>(12).unwrap_or(0) == 1,
        soft_nudge_fired: r.get::<_, i32>(13).unwrap_or(0) == 1,
    })
}

/// Sync today's soft-nudge schedule so **every** eligible task gets exactly one
/// random slot between 10:00–18:00 US CT (remaining window if already inside it).
/// Tasks with a hard alarm are scheduled strictly before that alarm.
/// Tasks already scheduled keep their slot.
pub fn sync_soft_nudge_plan(plan: SoftNudgePlan, eligible: &[SoftNudgeEligible]) -> SoftNudgePlan {
    let now = Utc::now().with_timezone(&Chicago);
    let date_ct = now.format("%Y-%m-%d").to_string();

    let window_start_h = 10u32;
    let window_end_h = 18u32;
    let day = now.date_naive();
    let day_start = Chicago
        .with_ymd_and_hms(day.year(), day.month(), day.day(), window_start_h, 0, 0)
        .single();
    let day_end = Chicago
        .with_ymd_and_hms(day.year(), day.month(), day.day(), window_end_h, 0, 0)
        .single();
    let (Some(day_start), Some(day_end)) = (day_start, day_end) else {
        return SoftNudgePlan {
            date_ct,
            slots: vec![],
        };
    };

    let eligible_ids: std::collections::HashSet<&str> =
        eligible.iter().map(|e| e.task_id.as_str()).collect();

    // New CT day or legacy plan without task_ids → rebuild from scratch.
    let mut slots: Vec<SoftNudgeSlot> = if plan.date_ct == date_ct
        && plan.slots.iter().all(|s| !s.task_id.is_empty())
    {
        plan.slots
            .into_iter()
            // Drop slots for tasks no longer eligible (done / alarm passed / deleted).
            .filter(|s| eligible_ids.contains(s.task_id.as_str()) || s.fired)
            .collect()
    } else {
        vec![]
    };

    if now >= day_end {
        // Past the window — keep fired history for the day, don't schedule new ones.
        return SoftNudgePlan { date_ct, slots };
    }

    let scheduled: std::collections::HashSet<String> =
        slots.iter().map(|s| s.task_id.clone()).collect();
    let mut missing: Vec<&SoftNudgeEligible> = eligible
        .iter()
        .filter(|e| !scheduled.contains(&e.task_id))
        .collect();

    if missing.is_empty() {
        return SoftNudgePlan { date_ct, slots };
    }

    // Shuffle so assignment order isn't checklist order.
    shuffle_eligibles(&mut missing);

    let range_start = if now > day_start { now } else { day_start };

    for (i, item) in missing.into_iter().enumerate() {
        let mut task_end = day_end;
        if let Some(before) = item.before {
            let before_ct = before.with_timezone(&Chicago);
            if before_ct < task_end {
                task_end = before_ct;
            }
        }
        if task_end <= range_start {
            continue;
        }
        let total_secs = (task_end - range_start).num_seconds().max(0) as u64;
        let at = if total_secs < 60 {
            range_start + chrono::Duration::seconds((i as i64) * 5)
        } else {
            let secs = (Utc::now().timestamp_subsec_nanos() as u64)
                .wrapping_mul((i as u64).wrapping_add(17))
                .wrapping_mul(1103515245)
                % total_secs;
            range_start + chrono::Duration::seconds(secs as i64)
        };
        slots.push(SoftNudgeSlot {
            at: at.min(task_end).to_rfc3339(),
            task_id: item.task_id.clone(),
            fired: false,
        });
    }

    SoftNudgePlan { date_ct, slots }
}

/// Human-readable alarm time for soft heads-up copy (e.g. "2:30 PM CT").
pub fn format_reminder_time_label(hour: u32, minute: u32, tz_name: &str) -> String {
    let h12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    let ampm = if hour < 12 { "AM" } else { "PM" };
    let zone = match normalize_tz(tz_name).as_str() {
        "America/Chicago" => " CT",
        "Asia/Karachi" => " PK",
        _ => "",
    };
    format!("{h12}:{minute:02} {ampm}{zone}")
}

fn shuffle_eligibles(items: &mut [&SoftNudgeEligible]) {
    let len = items.len();
    if len < 2 {
        return;
    }
    let mut seed = Utc::now().timestamp_subsec_nanos() as usize;
    for i in (1..len).rev() {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let j = seed % (i + 1);
        items.swap(i, j);
    }
}
