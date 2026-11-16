import { useEffect, useState } from "react";
import { api } from "../api";

type Props = {
  onDone: () => void;
};

export function OnboardingView({ onDone }: Props) {
  const [accessOk, setAccessOk] = useState<boolean | null>(null);
  const [automation, setAutomation] = useState<"ok" | "denied" | "unknown" | null>(
    null,
  );
  const [busy, setBusy] = useState(false);

  async function refresh() {
    const [a, auto] = await Promise.all([
      api.checkAccessibility().catch(() => false),
      api.checkAutomation().catch(() => "unknown" as const),
    ]);
    setAccessOk(a);
    setAutomation(auto);
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function finish() {
    setBusy(true);
    try {
      await api.completeOnboarding();
      onDone();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="onboarding">
      <h1>Welcome</h1>
      <p className="muted">
        Two macOS permissions keep tracking accurate. You can change these later
        in Settings.
      </p>

      <div className="onboarding-card">
        <div className="onboarding-row">
          <strong>Accessibility</strong>
          <span
            className={`onboarding-pill${
              accessOk === true ? " ok" : accessOk === false ? " bad" : ""
            }`}
          >
            {accessOk === null ? "…" : accessOk ? "OK" : "Needed"}
          </span>
        </div>
        <p className="muted">Lets the app see which app is frontmost.</p>
        <button type="button" onClick={() => void api.openAccessibilityPrefs()}>
          Open Accessibility settings
        </button>
      </div>

      <div className="onboarding-card">
        <div className="onboarding-row">
          <strong>Automation (Chrome)</strong>
          <span
            className={`onboarding-pill${
              automation === "ok"
                ? " ok"
                : automation === "denied"
                  ? " bad"
                  : ""
            }`}
          >
            {automation === null
              ? "…"
              : automation === "ok"
                ? "OK"
                : automation === "denied"
                  ? "Needed"
                  : "Open Chrome to test"}
          </span>
        </div>
        <p className="muted">
          Lets the app read the active tab URL so sites aren’t lumped as generic
          browsing.
        </p>
        <button type="button" onClick={() => void api.openAutomationPrefs()}>
          Open Automation settings
        </button>
      </div>

      <div className="onboarding-actions">
        <button type="button" className="ghost" onClick={() => void refresh()}>
          Re-check
        </button>
        <button type="button" disabled={busy} onClick={() => void finish()}>
          Continue
        </button>
      </div>
    </div>
  );
}
