import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { SoftNudgeBanner, SoftNudgeStyle } from "../types";
import { api } from "../api";
import { applyLiquidGlassVars, defaultGlassPrefs } from "../glassTheme";

function resolveStyle(banner: SoftNudgeBanner): SoftNudgeStyle {
  return banner.style === "liquidGlass" ? "liquidGlass" : "classic";
}

/** Compact toast anchored under the menu-bar status item. */
export function NudgeToast() {
  const [banner, setBanner] = useState<SoftNudgeBanner | null>(null);
  const [animKey, setAnimKey] = useState(0);
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    document.documentElement.classList.add("nudge-root");
    document.body.classList.add("nudge-root");
    return () => {
      document.documentElement.classList.remove("nudge-root");
      document.body.classList.remove("nudge-root");
    };
  }, []);

  useEffect(() => {
    if (!banner || resolveStyle(banner) !== "liquidGlass") return;
    applyLiquidGlassVars(document.documentElement, {
      glassLevel: banner.glassLevel ?? defaultGlassPrefs.glassLevel,
      glassReduceTransparency: banner.glassReduceTransparency ?? false,
      glassIncreaseContrast: banner.glassIncreaseContrast ?? false,
    });
    applyLiquidGlassVars(document.body, {
      glassLevel: banner.glassLevel ?? defaultGlassPrefs.glassLevel,
      glassReduceTransparency: banner.glassReduceTransparency ?? false,
      glassIncreaseContrast: banner.glassIncreaseContrast ?? false,
    });
  }, [banner]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const snap = await api.getToday();
        if (snap.softNudge) {
          setBanner(snap.softNudge);
          setExiting(false);
          setAnimKey((k) => k + 1);
        }
      } catch {
        /* ignore */
      }
      unlisten = await listen<SoftNudgeBanner>("soft-nudge", (event) => {
        setBanner(event.payload);
        setExiting(false);
        setAnimKey((k) => k + 1);
      });
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  async function dismiss() {
    if (exiting || !banner) return;
    setExiting(true);
    window.setTimeout(() => {
      void (async () => {
        setBanner(null);
        setExiting(false);
        try {
          await api.dismissSoftNudge();
        } catch {
          /* ignore */
        }
        try {
          await getCurrentWindow().hide();
        } catch {
          /* ignore */
        }
      })();
    }, 320);
  }

  if (!banner) {
    return <div className="nudge-viewport" aria-hidden />;
  }

  const style = resolveStyle(banner);

  return (
    <div
      key={animKey}
      className={`nudge-viewport nudge-style-${style}${exiting ? " is-exit" : " is-enter"}`}
      role="button"
      tabIndex={0}
      onClick={() => void dismiss()}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          void dismiss();
        }
      }}
      aria-label="Dismiss task nudge"
    >
      <div className="nudge-popover">
        <div className="nudge-popover-stem" aria-hidden />
        <div className="nudge-popover-body">
          <div className="nudge-glass-sheen" aria-hidden />
          <div className="nudge-popover-head">
            <span className="nudge-greeting">Hi Umer</span>
            <span className="nudge-close" aria-hidden>
              ✕
            </span>
          </div>
          {banner.kind === "alarmHeadsUp" ? (
            <p className="nudge-question">
              Just letting you know that task{" "}
              <span className="nudge-task">“{banner.summary}”</span> is set to
              be done at{" "}
              <span className="nudge-task">
                {banner.alarmTime ?? "its scheduled time"}
              </span>{" "}
              today.
            </p>
          ) : (
            <>
              <p className="nudge-question">
                Have you completed{" "}
                <span className="nudge-task">“{banner.summary}”</span> yet?
              </p>
              <p className="nudge-aside">
                Don’t forget before you sign off today.
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
