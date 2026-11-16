import { useEffect, useState } from "react";
import { TodayView } from "./components/TodayView";
import { SettingsView } from "./components/SettingsView";
import { OnboardingView } from "./components/OnboardingView";
import { api } from "./api";
import type { AppTheme, GlassPrefs } from "./types";
import {
  applyLiquidGlassVars,
  clearLiquidGlassVars,
  defaultGlassPrefs,
} from "./glassTheme";
import "./App.css";

type Page = "today" | "settings";

function resolveTheme(raw: string | undefined): AppTheme {
  return raw === "liquidGlass" ? "liquidGlass" : "legacy";
}

function prefsFromSettings(s: {
  glassLevel?: number;
  glassReduceTransparency?: boolean;
  glassIncreaseContrast?: boolean;
}): GlassPrefs {
  return {
    glassLevel: s.glassLevel ?? defaultGlassPrefs.glassLevel,
    glassReduceTransparency: s.glassReduceTransparency ?? false,
    glassIncreaseContrast: s.glassIncreaseContrast ?? false,
  };
}

export default function App() {
  const [page, setPage] = useState<Page>("today");
  const [theme, setTheme] = useState<AppTheme>("legacy");
  const [glassPrefs, setGlassPrefs] = useState<GlassPrefs>(defaultGlassPrefs);
  const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null);

  useEffect(() => {
    void api
      .getSettings()
      .then((s) => {
        setTheme(
          resolveTheme(
            s.appTheme ??
              (s.softNudgeStyle === "liquidGlass" ? "liquidGlass" : "legacy"),
          ),
        );
        setGlassPrefs(prefsFromSettings(s));
        setOnboardingDone(!!s.onboardingCompleted);
      })
      .catch(() => setOnboardingDone(true));
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    const body = document.body;
    root.dataset.theme = theme;
    body.dataset.theme = theme;
    body.classList.toggle("theme-liquidGlass", theme === "liquidGlass");
    body.classList.toggle("theme-legacy", theme === "legacy");
    if (theme === "liquidGlass") {
      applyLiquidGlassVars(body, glassPrefs);
      applyLiquidGlassVars(root, glassPrefs);
    } else {
      clearLiquidGlassVars(body);
      clearLiquidGlassVars(root);
    }
  }, [theme, glassPrefs]);

  if (onboardingDone === null) {
    return <div className="app-shell muted">Loading…</div>;
  }

  if (!onboardingDone) {
    return (
      <div className={`app-shell theme-${theme}`}>
        <OnboardingView onDone={() => setOnboardingDone(true)} />
      </div>
    );
  }

  return (
    <div className={`app-shell theme-${theme}`}>
      {page === "today" ? (
        <TodayView onOpenSettings={() => setPage("settings")} />
      ) : (
        <SettingsView
          onBack={() => setPage("today")}
          theme={theme}
          onThemeChange={setTheme}
          glassPrefs={glassPrefs}
          onGlassPrefsChange={setGlassPrefs}
        />
      )}
    </div>
  );
}
