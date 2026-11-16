import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type {
  AppSettings,
  AppTheme,
  CategoryRule,
  GlassPrefs,
  SoftNudgeBanner,
  TaskTemplate,
  TemplateDraft,
} from "../types";
import { DAY_NAMES, formatReminderLabel } from "../types";

type Props = {
  onBack: () => void;
  theme: AppTheme;
  onThemeChange: (theme: AppTheme) => void;
  glassPrefs: GlassPrefs;
  onGlassPrefsChange: (prefs: GlassPrefs) => void;
};

type DraftRow = TemplateDraft & { key: string };

function blankRow(): DraftRow {
  return {
    key: crypto.randomUUID(),
    summary: "",
    reminderEnabled: false,
    reminderHour: 18,
    reminderMinute: 0,
    reminderTz: "America/Chicago",
  };
}

function fromTemplate(t: TaskTemplate): DraftRow {
  return {
    key: t.id,
    summary: t.summary,
    reminderEnabled: !!t.reminderEnabled,
    reminderHour: t.reminderHour ?? 18,
    reminderMinute: t.reminderMinute ?? 0,
    reminderTz: "America/Chicago",
  };
}

export function SettingsView({
  onBack,
  theme,
  onThemeChange,
  glassPrefs,
  onGlassPrefsChange,
}: Props) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [templates, setTemplates] = useState<TaskTemplate[]>([]);
  const [rules, setRules] = useState<CategoryRule[]>([]);
  const [selectedDay, setSelectedDay] = useState(1);
  const [dayRows, setDayRows] = useState<DraftRow[]>([blankRow()]);
  const [status, setStatus] = useState<string | null>(null);
  const [accessOk, setAccessOk] = useState<boolean | null>(null);
  const [automation, setAutomation] = useState<
    "ok" | "denied" | "unknown" | null
  >(null);
  const [nudgeBanner, setNudgeBanner] = useState<SoftNudgeBanner | null>(null);

  useEffect(() => {
    void (async () => {
      const [s, t, r, a, auto] = await Promise.all([
        api.getSettings(),
        api.getTemplates(),
        api.getCategoryRules(),
        api.checkAccessibility().catch(() => false),
        api.checkAutomation().catch(() => "unknown" as const),
      ]);
      setSettings(s);
      setTemplates(t);
      setRules(r);
      setAccessOk(a);
      setAutomation(auto);
      try {
        const today = await api.getToday();
        setSelectedDay(today.clocks.workDayWeekday ?? 0);
      } catch {
        /* keep default */
      }
    })();
  }, []);

  useEffect(() => {
    const rows = templates
      .filter((t) => t.weekday === selectedDay)
      .sort((a, b) => a.sortOrder - b.sortOrder)
      .map(fromTemplate);
    setDayRows(rows.length ? rows : [blankRow()]);
  }, [templates, selectedDay]);

  const dayEnabled = useMemo(() => {
    if (!settings) return false;
    return settings.dayEnabled[selectedDay] ?? false;
  }, [settings, selectedDay]);

  async function persistSettings(next: AppSettings) {
    setSettings(next);
    await api.saveSettings(next);
    setStatus("Settings saved");
  }

  async function toggleDayEnabled() {
    if (!settings) return;
    const dayEnabledArr = [...settings.dayEnabled];
    while (dayEnabledArr.length < 7) dayEnabledArr.push(false);
    dayEnabledArr[selectedDay] = !dayEnabledArr[selectedDay];
    await persistSettings({ ...settings, dayEnabled: dayEnabledArr });
  }

  async function saveIdle(minutes: number) {
    if (!settings) return;
    await persistSettings({
      ...settings,
      idleThresholdMinutes: Math.max(1, Math.min(120, minutes)),
    });
  }

  function updateRow(key: string, patch: Partial<DraftRow>) {
    setDayRows((rows) =>
      rows.map((r) => (r.key === key ? { ...r, ...patch } : r)),
    );
  }

  async function saveDayTasks() {
    const payload: TemplateDraft[] = dayRows
      .map((r) => ({
        summary: r.summary.trim(),
        reminderEnabled: r.reminderEnabled,
        reminderHour: r.reminderEnabled ? r.reminderHour : null,
        reminderMinute: r.reminderEnabled ? r.reminderMinute : null,
        reminderTz: "America/Chicago",
      }))
      .filter((r) => r.summary);
    await api.saveDayTemplates(selectedDay, payload);
    const t = await api.getTemplates();
    setTemplates(t);
    setStatus(
      `Saved ${DAY_NAMES[selectedDay]} tasks (Today updates when that day is the current work day)`,
    );
  }

  async function toggleRule(id: string) {
    const next = rules.map((r) =>
      r.id === id ? { ...r, enabled: !r.enabled } : r,
    );
    setRules(next);
    await api.saveCategoryRules(next);
    setStatus("Categories updated");
  }

  if (!settings) {
    return <div className="panel muted">Loading settings…</div>;
  }

  return (
    <div className="settings">
      <header className="topbar">
        <button type="button" className="ghost" onClick={onBack}>
          ← Today
        </button>
        <h1>Settings</h1>
      </header>

      {status ? <p className="status">{status}</p> : null}

      <section className="section">
        <details className="log-fold">
          <summary className="log-summary">
            <span className="log-summary-title">Appearance</span>
            <span className="log-chevron" aria-hidden />
          </summary>
          <p className="muted">
            Choose how the app and soft-nudge toasts look. Legacy keeps the
            original colors; Liquid Glass applies a frosted glass look throughout.
          </p>
          <div className="style-seg" role="group" aria-label="App theme">
            {(
              [
                ["legacy", "Legacy"],
                ["liquidGlass", "Liquid Glass"],
              ] as const
            ).map(([value, label]) => {
              const active = theme === value;
              return (
                <button
                  key={value}
                  type="button"
                  className={active ? "is-active" : ""}
                  onClick={() => {
                    void (async () => {
                      const nextTheme = value as AppTheme;
                      onThemeChange(nextTheme);
                      await persistSettings({
                        ...settings,
                        appTheme: nextTheme,
                        glassLevel: glassPrefs.glassLevel,
                        glassReduceTransparency:
                          glassPrefs.glassReduceTransparency,
                        glassIncreaseContrast: glassPrefs.glassIncreaseContrast,
                      });
                      setStatus(`Theme: ${label}`);
                      if (settings.softNudgesEnabled !== false) {
                        try {
                          const banner = await api.previewSoftNudge();
                          setNudgeBanner(banner);
                        } catch {
                          /* ignore */
                        }
                      }
                    })();
                  }}
                >
                  {label}
                </button>
              );
            })}
          </div>

          {theme === "liquidGlass" ? (
            <div className="glass-controls">
              <p className="glass-controls-title">Liquid Glass</p>
              <p className="muted glass-controls-hint">
                Same idea as macOS 27 Appearance: Clear on the left, Frosted on
                the right. Use the toggles if text is hard to read.
              </p>
              <div className="glass-slider-row">
                <span className="glass-slider-label">Clear</span>
                <input
                  className="glass-slider"
                  type="range"
                  min={0}
                  max={100}
                  step={1}
                  value={glassPrefs.glassLevel}
                  aria-label="Liquid Glass level"
                  onChange={(e) => {
                    const glassLevel = Number(e.target.value);
                    const next = { ...glassPrefs, glassLevel };
                    onGlassPrefsChange(next);
                    void persistSettings({
                      ...settings,
                      appTheme: "liquidGlass",
                      ...next,
                    });
                  }}
                />
                <span className="glass-slider-label">Frosted</span>
              </div>
              <p className="muted glass-slider-value">
                Level {glassPrefs.glassLevel}
                {glassPrefs.glassLevel < 35
                  ? " · more glassy"
                  : glassPrefs.glassLevel > 70
                    ? " · more opaque"
                    : " · balanced"}
              </p>
              <label className="toggle-row">
                <input
                  type="checkbox"
                  checked={glassPrefs.glassReduceTransparency}
                  onChange={() => {
                    const next = {
                      ...glassPrefs,
                      glassReduceTransparency:
                        !glassPrefs.glassReduceTransparency,
                    };
                    onGlassPrefsChange(next);
                    void persistSettings({
                      ...settings,
                      appTheme: "liquidGlass",
                      ...next,
                    });
                  }}
                />
                Reduce transparency
              </label>
              <label className="toggle-row">
                <input
                  type="checkbox"
                  checked={glassPrefs.glassIncreaseContrast}
                  onChange={() => {
                    const next = {
                      ...glassPrefs,
                      glassIncreaseContrast: !glassPrefs.glassIncreaseContrast,
                    };
                    onGlassPrefsChange(next);
                    void persistSettings({
                      ...settings,
                      appTheme: "liquidGlass",
                      ...next,
                    });
                  }}
                />
                Increase contrast
              </label>
              <button
                type="button"
                className="ghost glass-preview-btn"
                onClick={() =>
                  void (async () => {
                    try {
                      const banner = await api.previewSoftNudge();
                      setNudgeBanner(banner);
                      setStatus("Previewing toast with current glass settings");
                    } catch (e) {
                      setStatus(String(e));
                    }
                  })()
                }
              >
                Preview toast with these settings
              </button>
            </div>
          ) : null}
        </details>
      </section>

      <section className="section">
        <h2>Idle threshold</h2>
        <p className="muted">
          Time without keyboard/mouse before counting as idle.
        </p>
        <div className="add-row">
          <input
            className="field narrow"
            type="number"
            min={1}
            max={120}
            value={settings.idleThresholdMinutes}
            onChange={(e) =>
              setSettings({
                ...settings,
                idleThresholdMinutes: Number(e.target.value) || 5,
              })
            }
          />
          <span className="muted">minutes</span>
          <button
            type="button"
            onClick={() => void saveIdle(settings.idleThresholdMinutes)}
          >
            Save
          </button>
        </div>
      </section>

      <section className="section">
        <details className="log-fold">
          <summary className="log-summary">
            <span className="log-summary-title">Permissions</span>
            <span className="log-chevron" aria-hidden />
          </summary>
          <p className="muted">
            Two separate macOS permissions are needed for accurate tracking.
          </p>
          <p>
            Accessibility:{" "}
            {accessOk === null
              ? "…"
              : accessOk
                ? "OK — can detect frontmost app"
                : "Limited — grant Accessibility"}
          </p>
          <button type="button" onClick={() => void api.openAccessibilityPrefs()}>
            Open Accessibility settings
          </button>
          <p style={{ marginTop: "0.75rem" }}>
            Automation (Chrome tab URLs):{" "}
            {automation === null
              ? "…"
              : automation === "ok"
                ? "OK — can read site domains"
                : automation === "denied"
                  ? "Denied — Zendesk/support sites may show as Browsing"
                  : "Unknown — open Chrome on a tab, then refresh Settings"}
          </p>
          <button type="button" onClick={() => void api.openAutomationPrefs()}>
            Open Automation settings
          </button>
          <button
            type="button"
            className="ghost"
            style={{ marginLeft: "0.5rem" }}
            onClick={() =>
              void (async () => {
                const auto = await api.checkAutomation().catch(() => "unknown" as const);
                setAutomation(auto);
                setAccessOk(await api.checkAccessibility().catch(() => false));
              })()
            }
          >
            Re-check
          </button>
        </details>
      </section>

      <section className="section">
        <h2>Soft task nudges</h2>
        <p className="muted">
          Between <strong>10:00 AM – 6:00 PM US CT</strong>, every incomplete
          task gets exactly one soft nudge at a random time. Tasks without an
          alarm get a gentle “don’t forget” nudge; tasks with an alarm still
          ahead get a heads-up that they’re set for that time (before the hard
          alarm). Nudges slide down from your menu-bar status item (e.g.{" "}
          <strong>0/4 · …</strong>) — click the toast to dismiss.
        </p>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={settings.softNudgesEnabled !== false}
            onChange={() => {
              void (async () => {
                const nextEnabled = !(settings.softNudgesEnabled !== false);
                await persistSettings({
                  ...settings,
                  softNudgesEnabled: nextEnabled,
                });
                if (nextEnabled) {
                  try {
                    const banner = await api.previewSoftNudge();
                    setNudgeBanner(banner);
                    setStatus(
                      `Preview: “${banner.summary}” — look near the menu bar (top-right)`,
                    );
                  } catch (e) {
                    setNudgeBanner(null);
                    setStatus(String(e));
                  }
                } else {
                  setNudgeBanner(null);
                  await api.dismissSoftNudge().catch(() => undefined);
                }
              })();
            }}
          />
          Enable soft random nudges
        </label>
        {nudgeBanner ? (
          <div className="soft-nudge-banner" role="status">
            <div className="soft-nudge-title">Hi Umer!</div>
            <div className="soft-nudge-body">{nudgeBanner.message}</div>
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setNudgeBanner(null);
                void api.dismissSoftNudge();
              }}
            >
              Dismiss
            </button>
          </div>
        ) : null}
        <button
          type="button"
          onClick={() =>
            void (async () => {
              try {
                const banner = await api.previewSoftNudge();
                setNudgeBanner(banner);
                setStatus(
                  `Preview: “${banner.summary}” — toast near the menu bar`,
                );
              } catch (e) {
                setStatus(String(e));
              }
            })()
          }
        >
          Preview one soft nudge now
        </button>
      </section>

      <section className="section">
        <h2>Daily task templates</h2>
        <p className="muted">
          Day tabs follow the <strong>work day</strong> (starts{" "}
          <strong>5:00 PM Pakistan</strong>), not Pakistan’s midnight calendar —
          so after midnight PK you may still be on Thursday’s checklist while
          the Pakistan clock already says Friday. Alarms are set in{" "}
          <strong>US Central</strong> only (PK equivalent shown next to them).
          Saving a day also updates <strong>Today</strong> when that day is the
          current work day; other days apply next time they come around.
        </p>
        <div className="day-tabs">
          {DAY_NAMES.map((name, i) => (
            <button
              key={name}
              type="button"
              className={
                i === selectedDay
                  ? "day-tab active"
                  : settings.dayEnabled[i]
                    ? "day-tab"
                    : "day-tab off"
              }
              onClick={() => setSelectedDay(i)}
            >
              {name.slice(0, 3)}
            </button>
          ))}
        </div>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={dayEnabled}
            onChange={() => void toggleDayEnabled()}
          />
          Generate checklist on {DAY_NAMES[selectedDay]}
        </label>

        <ul className="template-editor">
          {dayRows.map((row) => (
            <li key={row.key} className="template-row">
              <input
                className="field"
                placeholder="Task name"
                value={row.summary}
                onChange={(e) => updateRow(row.key, { summary: e.target.value })}
              />
              <label className="alarm-toggle">
                <input
                  type="checkbox"
                  checked={row.reminderEnabled}
                  onChange={(e) =>
                    updateRow(row.key, { reminderEnabled: e.target.checked })
                  }
                />
                Alarm
              </label>
              {row.reminderEnabled ? (
                <div className="alarm-fields">
                  <span className="muted">US CT</span>
                  <input
                    className="field narrow"
                    type="number"
                    min={0}
                    max={23}
                    value={row.reminderHour ?? 0}
                    onChange={(e) =>
                      updateRow(row.key, {
                        reminderHour: Math.min(
                          23,
                          Math.max(0, Number(e.target.value) || 0),
                        ),
                      })
                    }
                    title="Hour US CT (0–23)"
                  />
                  <span>:</span>
                  <input
                    className="field narrow"
                    type="number"
                    min={0}
                    max={59}
                    value={row.reminderMinute ?? 0}
                    onChange={(e) =>
                      updateRow(row.key, {
                        reminderMinute: Math.min(
                          59,
                          Math.max(0, Number(e.target.value) || 0),
                        ),
                      })
                    }
                    title="Minute"
                  />
                  <span className="muted alarm-preview">
                    {formatReminderLabel(
                      row.reminderHour,
                      row.reminderMinute,
                    )}
                  </span>
                </div>
              ) : null}
              <button
                type="button"
                className="ghost danger-text"
                onClick={() =>
                  setDayRows((rows) =>
                    rows.length <= 1
                      ? [blankRow()]
                      : rows.filter((r) => r.key !== row.key),
                  )
                }
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
        <div className="add-row">
          <button type="button" className="ghost" onClick={() => setDayRows((r) => [...r, blankRow()])}>
            + Add task
          </button>
          <button type="button" onClick={() => void saveDayTasks()}>
            Save {DAY_NAMES[selectedDay]} tasks
          </button>
        </div>
      </section>

      <section className="section">
        <details className="log-fold">
          <summary className="log-summary">
            <span className="log-summary-title">
              Categories
              <span className="badge">{rules.length}</span>
            </span>
            <span className="log-chevron" aria-hidden />
          </summary>
          <p className="muted">
            Enable or disable tracked apps and sites. New apps are added
            automatically when detected; Other is only used when the frontmost
            app cannot be read.
          </p>
          <ul className="rule-list">
            {rules.map((r) => (
              <li key={r.id}>
                <label className="toggle-row">
                  <input
                    type="checkbox"
                    checked={r.enabled}
                    onChange={() => void toggleRule(r.id)}
                  />
                  <span>
                    <strong>{r.label}</strong>
                    <span className="muted"> — {r.display}</span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        </details>
      </section>

      <p className="muted footer-note">
        All data stays on this Mac (local SQLite). No account, no sync.
      </p>
    </div>
  );
}
