import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { TaskInstance, TodaySnapshot } from "../types";
import { formatDuration, formatLogTime, formatReminderLabel } from "../types";

type Props = {
  onOpenSettings: () => void;
};

export function TodayView({ onOpenSettings }: Props) {
  const [snap, setSnap] = useState<TodaySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [newSummary, setNewSummary] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const data = await api.getToday();
      setSnap(data);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), 5000);
    let unlisten: (() => void) | undefined;
    void listen("tracking-changed", () => {
      void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      window.clearInterval(id);
      unlisten?.();
    };
  }, [refresh]);

  async function toggleDone(task: TaskInstance) {
    const next = {
      ...task,
      status: task.status === "done" ? "todo" : "done",
    };
    setBusy(true);
    try {
      await api.updateTask(next);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function saveNotes(task: TaskInstance, notes: string, ticketLink: string) {
    setBusy(true);
    try {
      await api.updateTask({ ...task, notes, ticketLink });
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function removeTask(id: string) {
    setBusy(true);
    try {
      await api.deleteTask(id);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function addTask() {
    const summary = newSummary.trim();
    if (!summary) return;
    setBusy(true);
    try {
      await api.addTask(summary);
      setNewSummary("");
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function toggleTracking() {
    if (!snap) return;
    setBusy(true);
    try {
      const next = await api.setTrackingEnabled(!snap.trackingEnabled);
      setSnap(next);
    } finally {
      setBusy(false);
    }
  }

  if (error) {
    return (
      <div className="panel">
        <p className="error">{error}</p>
        <button type="button" onClick={() => void refresh()}>
          Retry
        </button>
      </div>
    );
  }

  if (!snap) {
    return <div className="panel muted">Loading…</div>;
  }

  const maxCat = Math.max(
    1,
    ...snap.summary.byCategory.map((c) => c.seconds),
  );

  return (
    <div className="today">
      {snap.softNudge ? (
        <div className="soft-nudge-banner" role="status">
          <div className="soft-nudge-title">Hi Umer!</div>
          <div className="soft-nudge-body">{snap.softNudge.message}</div>
          <button
            type="button"
            className="ghost"
            onClick={() =>
              void (async () => {
                await api.dismissSoftNudge();
                await refresh();
              })()
            }
          >
            Dismiss
          </button>
        </div>
      ) : null}
      <header className="topbar">
        <div>
          <h1>Today</h1>
          <div className="clocks">
            <div className="clock-row">
              <span className="clock-label">Pakistan</span>
              <span>
                {snap.clocks.pakistan.weekday} · {snap.clocks.pakistan.date} ·{" "}
                {snap.clocks.pakistan.time}
              </span>
            </div>
            <div className="clock-row primary">
              <span className="clock-label">US Central</span>
              <span>
                {snap.clocks.usCentral.weekday} · {snap.clocks.usCentral.date} ·{" "}
                {snap.clocks.usCentral.time}
              </span>
            </div>
            <p className="date-line">
              Work day: {snap.clocks.workDayLabel}
            </p>
          </div>
        </div>
        <button type="button" className="ghost" onClick={onOpenSettings}>
          Settings
        </button>
      </header>

      <section
        className={`now-card${
          !snap.trackingEnabled
            ? " stopped"
            : snap.current.categoryId === "paused"
              ? " paused"
              : " running"
        }`}
      >
        <div className="track-status-row">
          <div
            className={`track-status${
              !snap.trackingEnabled
                ? " is-stopped"
                : snap.current.categoryId === "paused"
                  ? " is-paused"
                  : " is-running"
            }`}
            aria-live="polite"
          >
            <span className="track-dot" aria-hidden />
            <span className="track-status-text">
              {!snap.trackingEnabled
                ? "Tracking is OFF"
                : snap.current.categoryId === "paused"
                  ? `Paused · ${snap.current.pauseReason ?? "System"}`
                  : "Tracking is ON"}
            </span>
          </div>
        </div>

        <div className="now-label">
          {!snap.trackingEnabled
            ? "Not detecting"
            : snap.current.categoryId === "paused"
              ? "Auto-paused"
              : snap.current.isIdle
                ? "Away"
                : "Currently detecting"}
        </div>
        <div className="now-display">
          {snap.trackingEnabled
            ? snap.current.isIdle
              ? "Idle"
              : snap.current.display
            : "Stopped"}
        </div>
        {snap.trackingEnabled &&
        !snap.current.isIdle &&
        snap.current.categoryId !== "paused" &&
        snap.current.windowTitle ? (
          <div className="now-title" title={snap.current.windowTitle}>
            {snap.current.windowTitle}
          </div>
        ) : null}
        <div className="now-meta">
          {!snap.trackingEnabled
            ? "Nothing is being recorded until you start"
            : snap.current.categoryId === "paused"
              ? "Resumes when the display is on and unlocked (clamshell OK)"
              : snap.current.isIdle
                ? `Away ${formatDuration(snap.current.idleSeconds)}`
                : snap.current.appName || "—"}
        </div>

        <button
          type="button"
          className={`track-btn${snap.trackingEnabled ? " stop" : " start"}`}
          disabled={busy}
          onClick={() => void toggleTracking()}
        >
          {snap.trackingEnabled ? "Stop tracking" : "Start tracking"}
        </button>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Tasks</h2>
          <span className="badge">
            {snap.tasksDone}/{snap.tasksTotal}
          </span>
        </div>
        {snap.tasks.some(
          (t) => t.reminderEnabled && t.reminderFired && t.status !== "done",
        ) ? (
          <div className="due-banner">
            Reminder fired — check:{" "}
            {snap.tasks
              .filter(
                (t) =>
                  t.reminderEnabled && t.reminderFired && t.status !== "done",
              )
              .map((t) => t.summary)
              .join(" · ")}
          </div>
        ) : null}
        {snap.tasks.length === 0 ? (
          <p className="muted">
            No checklist for this day. Enable the day or add tasks in Settings.
          </p>
        ) : (
          <ul className="task-list">
            {snap.tasks.map((task) => (
              <li key={task.id} className={task.status === "done" ? "done" : ""}>
                <label className="task-row">
                  <input
                    type="checkbox"
                    checked={task.status === "done"}
                    disabled={busy}
                    onChange={() => void toggleDone(task)}
                  />
                  <span className="task-summary">{task.summary}</span>
                  {task.reminderEnabled ? (
                    <span
                      className={`alarm-badge${task.reminderFired ? " fired" : ""}`}
                      title={
                        task.reminderFired
                          ? "Reminder already shown"
                          : "Alarm set"
                      }
                    >
                      {formatReminderLabel(
                        task.reminderHour,
                        task.reminderMinute,
                        task.reminderTz,
                      )}
                    </span>
                  ) : null}
                  <button
                    type="button"
                    className="ghost danger-text"
                    disabled={busy}
                    title="Delete task"
                    onClick={(e) => {
                      e.preventDefault();
                      void removeTask(task.id);
                    }}
                  >
                    Delete
                  </button>
                </label>
                <details className="task-details">
                  <summary>Notes / link / alarm</summary>
                  <input
                    className="field"
                    placeholder="Ticket link"
                    defaultValue={task.ticketLink}
                    onBlur={(e) => {
                      if (e.target.value !== task.ticketLink) {
                        void saveNotes(task, task.notes, e.target.value);
                      }
                    }}
                  />
                  <textarea
                    className="field"
                    placeholder="Resubmits / notes"
                    rows={2}
                    defaultValue={task.notes}
                    onBlur={(e) => {
                      if (e.target.value !== task.notes) {
                        void saveNotes(task, e.target.value, task.ticketLink);
                      }
                    }}
                  />
                  <label className="alarm-toggle">
                    <input
                      type="checkbox"
                      checked={task.reminderEnabled}
                      disabled={busy}
                      onChange={(e) => {
                        void (async () => {
                          setBusy(true);
                          try {
                            await api.updateTask({
                              ...task,
                              reminderEnabled: e.target.checked,
                              reminderHour: task.reminderHour ?? 18,
                              reminderMinute: task.reminderMinute ?? 0,
                              reminderTz: "America/Chicago",
                              reminderFired: e.target.checked
                                ? false
                                : task.reminderFired,
                            });
                            await refresh();
                          } finally {
                            setBusy(false);
                          }
                        })();
                      }}
                    />
                    Alarm for this task
                  </label>
                  {task.reminderEnabled ? (
                    <div className="alarm-fields">
                      <span className="muted">US CT</span>
                      <input
                        className="field narrow"
                        type="number"
                        min={0}
                        max={23}
                        value={task.reminderHour ?? 18}
                        disabled={busy}
                        onChange={(e) => {
                          const hour = Math.min(
                            23,
                            Math.max(0, Number(e.target.value) || 0),
                          );
                          void (async () => {
                            setBusy(true);
                            try {
                              await api.updateTask({
                                ...task,
                                reminderHour: hour,
                                reminderTz: "America/Chicago",
                                reminderFired: false,
                              });
                              await refresh();
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      />
                      <span>:</span>
                      <input
                        className="field narrow"
                        type="number"
                        min={0}
                        max={59}
                        value={task.reminderMinute ?? 0}
                        disabled={busy}
                        onChange={(e) => {
                          const minute = Math.min(
                            59,
                            Math.max(0, Number(e.target.value) || 0),
                          );
                          void (async () => {
                            setBusy(true);
                            try {
                              await api.updateTask({
                                ...task,
                                reminderMinute: minute,
                                reminderTz: "America/Chicago",
                                reminderFired: false,
                              });
                              await refresh();
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      />
                      <span className="muted alarm-preview">
                        {formatReminderLabel(
                          task.reminderHour,
                          task.reminderMinute,
                        )}
                      </span>
                    </div>
                  ) : null}
                </details>
              </li>
            ))}
          </ul>
        )}
        <div className="add-row">
          <input
            className="field"
            placeholder="Add ad-hoc task…"
            value={newSummary}
            onChange={(e) => setNewSummary(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addTask();
            }}
          />
          <button type="button" disabled={busy || !newSummary.trim()} onClick={() => void addTask()}>
            Add
          </button>
        </div>
      </section>

      <section className="section">
        <div className="section-head">
          <h2>Time today</h2>
        </div>
        <div className="time-totals">
          <div>
            <span className="muted">Active</span>
            <strong>{formatDuration(snap.summary.activeSeconds)}</strong>
          </div>
          <div>
            <span className="muted">Idle</span>
            <strong>{formatDuration(snap.summary.idleSeconds)}</strong>
          </div>
        </div>
        <div className="stat-grid">
          <div className="stat-chip productive">
            <span>Productive</span>
            <strong>
              {formatDuration(
                snap.summary.productiveSeconds +
                  (snap.summary.browsingSeconds ?? 0),
              )}
            </strong>
          </div>
          <div className="stat-chip entertainment">
            <span>Entertainment</span>
            <strong>{formatDuration(snap.summary.entertainmentSeconds)}</strong>
          </div>
          <div className="stat-chip communication">
            <span>Communication</span>
            <strong>{formatDuration(snap.summary.communicationSeconds)}</strong>
          </div>
          <div className="stat-chip other">
            <span>Other</span>
            <strong>{formatDuration(snap.summary.otherSeconds)}</strong>
          </div>
        </div>
        {snap.summary.byCategory.length === 0 ? (
          <p className="muted">
            Hit Start to begin logging. Only time while tracking is ON counts.
          </p>
        ) : (
          <details className="log-fold app-bars-fold">
            <summary className="log-summary">
              <span className="log-summary-title app-bars-title">
                Apps &amp; sites
                <span className="badge">{snap.summary.byCategory.length}</span>
              </span>
              <span className="log-chevron" aria-hidden />
            </summary>
            <ul className="bars">
              {snap.summary.byCategory
                .slice()
                .sort((a, b) => b.seconds - a.seconds)
                .map((c) => (
                  <li key={c.categoryId}>
                    <div className="bar-label">
                      <span>{c.categoryLabel}</span>
                      <span>{formatDuration(c.seconds)}</span>
                    </div>
                    <div className="bar-track">
                      <div
                        className="bar-fill"
                        style={{ width: `${(c.seconds / maxCat) * 100}%` }}
                      />
                    </div>
                  </li>
                ))}
            </ul>
          </details>
        )}
      </section>

      <section className="section log-section">
        <details className="log-fold">
          <summary className="log-summary">
            <span className="log-summary-title">
              Activity log
              <span className="badge">{snap.activityLog.length}</span>
            </span>
            <span className="log-chevron" aria-hidden />
          </summary>
          <p className="muted log-hint">
          Start/Stop markers plus each detected change. Times in Pakistan.
        </p>
          {snap.activityLog.length === 0 ? (
            <p className="muted">No activity logged for this work day yet.</p>
          ) : (
            <ul className="activity-log">
              {[...snap.activityLog].reverse().map((entry) => (
                <li key={entry.id} className={`log-${entry.kind}`}>
                  <div className="log-time">
                    {formatLogTime(entry.startedAt)}
                    {entry.endedAt ? ` – ${formatLogTime(entry.endedAt)}` : ""}
                  </div>
                  <div className="log-label">{entry.label}</div>
                  {entry.detail ? (
                    <div className="log-detail" title={entry.detail}>
                      {entry.detail}
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </details>
      </section>
    </div>
  );
}
