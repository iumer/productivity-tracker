import type { GlassPrefs } from "./types";

/** Map macOS 27–style glass prefs → CSS variables on an element. */
export function applyLiquidGlassVars(
  el: HTMLElement,
  prefs: GlassPrefs,
): void {
  const level = Math.max(0, Math.min(100, prefs.glassLevel ?? 55));
  let frost = level / 100;
  if (prefs.glassReduceTransparency) {
    frost = Math.max(frost, 0.9);
  }

  // Higher frost = denser fill, stronger blur, less see-through.
  const fill = 0.38 + frost * 0.54; // ~0.38 clear → ~0.92 frosted
  const fillTop = Math.min(0.97, fill + 0.06);
  const blur = 16 + frost * 28; // 16–44px
  const sat = 150 + frost * 40;
  const border = 0.45 + frost * 0.35;
  const shadow = 0.1 + frost * 0.18;

  const contrast = prefs.glassIncreaseContrast;
  const ink = contrast ? "#050a12" : "#0b1220";
  const muted = contrast
    ? "rgba(5, 10, 18, 0.78)"
    : "rgba(11, 18, 32, 0.64)";
  const accent = contrast ? "#084556" : "#0f5f78";
  const task = contrast ? "#06394a" : "#0a4d63";
  const weight = contrast ? "700" : "600";
  const asideWeight = contrast ? "600" : "500";

  el.style.setProperty("--glass-fill", fill.toFixed(3));
  el.style.setProperty("--glass-fill-top", fillTop.toFixed(3));
  el.style.setProperty("--glass-blur", `${blur.toFixed(0)}px`);
  el.style.setProperty("--glass-sat", `${sat.toFixed(0)}%`);
  el.style.setProperty("--glass-border", border.toFixed(3));
  el.style.setProperty("--glass-shadow", shadow.toFixed(3));
  el.style.setProperty("--glass-ink", ink);
  el.style.setProperty("--glass-muted", muted);
  el.style.setProperty("--glass-accent", accent);
  el.style.setProperty("--glass-task", task);
  el.style.setProperty("--glass-weight", weight);
  el.style.setProperty("--glass-aside-weight", asideWeight);
  el.dataset.glassLevel = String(level);
  el.dataset.glassReduce = prefs.glassReduceTransparency ? "1" : "0";
  el.dataset.glassContrast = prefs.glassIncreaseContrast ? "1" : "0";
}

export function clearLiquidGlassVars(el: HTMLElement): void {
  const keys = [
    "--glass-fill",
    "--glass-fill-top",
    "--glass-blur",
    "--glass-sat",
    "--glass-border",
    "--glass-shadow",
    "--glass-ink",
    "--glass-muted",
    "--glass-accent",
    "--glass-task",
    "--glass-weight",
    "--glass-aside-weight",
  ];
  for (const k of keys) el.style.removeProperty(k);
  delete el.dataset.glassLevel;
  delete el.dataset.glassReduce;
  delete el.dataset.glassContrast;
}

export const defaultGlassPrefs: GlassPrefs = {
  glassLevel: 55,
  glassReduceTransparency: false,
  glassIncreaseContrast: false,
};
