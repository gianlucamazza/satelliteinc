/** Shared HTML helpers for the mission-control HUD (no framework). */

/** Escape content-derived strings interpolated into innerHTML. */
export function esc(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Stat cell with a fill bar showing value against capacity. */
export function gauge(
  label: string,
  value: number,
  capacity: number,
  unit: string,
  opts: { lowIsBad?: boolean; highIsWarn?: boolean } = {},
): string {
  const pct =
    capacity > 0 ? Math.max(0, Math.min(100, (value / capacity) * 100)) : 0;
  const level =
    opts.lowIsBad && pct < 20
      ? " danger"
      : opts.highIsWarn && pct > 80
        ? " warn"
        : "";
  return `<div class="stat gauge">
    <label>${label}</label>
    <strong>${value.toFixed(value < 10 ? 1 : 0)} ${unit} <span class="stat-sub">(${pct.toFixed(0)}%)</span></strong>
    <div class="progress-bar thin"><div class="progress-fill${level}" style="width:${pct}%"></div></div>
  </div>`;
}
