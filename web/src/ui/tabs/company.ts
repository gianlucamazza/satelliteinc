import type { GameContent, Snapshot } from "../../types.ts";
import { esc } from "../html.ts";

export interface CompanyTabPrefs {
  autoDownlink: boolean;
  autoPauseArrival: boolean;
  autoPauseAnomaly: boolean;
  showTleRefs: boolean;
  showLegend: boolean;
}

/** Company tab: programs, mode, prefs, R&D tree. */
export function renderCompanyTab(
  s: Snapshot,
  content: GameContent,
  prefs: CompanyTabPrefs,
): string[] {
  const parts: string[] = [];

  const pr = s.prestige;
  if (pr) {
    const next =
      pr.next_rank_at != null
        ? ` · next rank at ${pr.next_rank_at}`
        : " · max rank";
    const goals = pr.goals
      .map(
        (g) =>
          `<li class="${g.done ? "goal-done" : ""}">${g.done ? "✓" : "○"} ${esc(g.label)}</li>`,
      )
      .join("");
    parts.push(`
      <div class="field prestige-panel">
        <label>Agency prestige</label>
        <div class="stat"><label>Rank</label><strong>${esc(pr.rank_label)}</strong>
          <span class="stat-sub">score ${pr.score}${next}</span></div>
        <div class="stat-grid">
          <div class="stat"><label>Contracts</label><strong>${pr.contracts_completed}</strong></div>
          <div class="stat"><label>Programs</label><strong>${pr.programs_completed}</strong></div>
          <div class="stat"><label>Contests won</label><strong>${pr.contested_wins}</strong></div>
        </div>
        <ul class="goal-list">${goals}</ul>
        <p class="hint">Career score: contracts×15 · programs×120 · contests×25 · rep×2 · cash & revenue · grand victory +250.</p>
      </div>
    `);
  }

  // Career spine — always visible (progress board even in sandbox lab).
  if (s.programs.length > 0) {
    const rows = s.programs
      .map((p) => {
        const canBuy =
          s.game_mode === "campaign" &&
          !p.unlocked &&
          !p.completed &&
          s.reputation >= p.reputation_required &&
          p.keystone_done &&
          p.tech_done &&
          s.cash_usd >= p.buyin_usd;
        const status = p.completed
          ? `<strong class="pass-active">COMPLETE</strong>`
          : p.unlocked
            ? `<strong class="pass-active">ACTIVE</strong>`
            : s.game_mode === "campaign"
              ? `<button class="btn-unlock-program" data-program="${esc(p.id)}"
                  ${canBuy ? "" : "disabled"}
                  title="${!p.keystone_done ? `Complete ${p.requires_keystone ?? ""} first` : !p.tech_done ? `Research ${p.requires_tech ?? ""} first` : s.reputation < p.reputation_required ? `Requires reputation ${p.reputation_required}` : s.cash_usd < p.buyin_usd ? "Insufficient funds" : "Buy into the program"}">
                  Unlock — $${p.buyin_usd.toLocaleString()}
                </button>`
              : `<strong class="muted">LOCKED</strong>`;
        return `<div class="stat"><label>${esc(p.name)}</label>${status}<span class="stat-sub">rep ${p.reputation_required}${p.completion_grant_usd > 0 ? ` · grant $${(p.completion_grant_usd / 1000).toFixed(0)}k` : ""}</span></div>`;
      })
      .join("");
    parts.push(
      `<div class="field"><label>Career path</label>${rows}<p class="hint">One career: Earth → Moon → Mars → Venus → Jupiter. Each program opens a body; its keystone gates the next.</p></div>`,
    );
  }

  const careerId =
    content.scenarios?.find((sc) => sc.game_mode === "campaign")?.id ?? "career";
  const sandboxId =
    content.scenarios?.find((sc) => sc.game_mode === "sandbox")?.id ?? "sandbox";
  parts.push(`
    <div class="field"><label>Play mode</label>
      <p class="hint mode-badge">${s.game_mode === "campaign" ? "Career — rep gates, rivals & weather scale with programs, grand victory." : "Sandbox lab — all unlocks, no bankruptcy freeze, market/weather off."}</p>
      <div class="row play-mode-row">
        <button type="button" id="btn-new-career" class="primary" data-scenario="${esc(careerId)}" title="Hard reset into a new Career">New Career</button>
        <button type="button" id="btn-new-sandbox" data-scenario="${esc(sandboxId)}" title="Hard reset into Sandbox lab">New Sandbox</button>
      </div>
      ${
        s.game_mode === "campaign"
          ? `<button type="button" id="btn-enter-sandbox-mode" class="ghost">Continue as Sandbox lab</button>
             <p class="hint">Keeps this run but opens all unlocks and quiets market/weather.</p>`
          : `<p class="hint">To play Career rules, start a New Career (clean slate).</p>`
      }
    </div>
    <div class="field"><label>Preferences</label></div>
    <label class="toggle">
      <input type="checkbox" id="pref-auto-downlink" ${prefs.autoDownlink ? "checked" : ""} />
      Auto-downlink on pass
    </label>
    <label class="toggle">
      <input type="checkbox" id="pref-auto-pause-arrival" ${prefs.autoPauseArrival ? "checked" : ""} />
      Auto-pause on transfer arrival
    </label>
    <label class="toggle">
      <input type="checkbox" id="pref-auto-pause-anomaly" ${prefs.autoPauseAnomaly ? "checked" : ""} />
      Auto-pause on anomaly
    </label>
    <label class="toggle">
      <input type="checkbox" id="show-tle" ${prefs.showTleRefs ? "checked" : ""} />
      Show TLE reference satellites (ISS)
    </label>
    <label class="toggle">
      <input type="checkbox" id="show-legend" ${prefs.showLegend ? "checked" : ""} />
      Show globe legend
    </label>
    <button id="btn-export-save">Save game (download)</button>
    <button id="btn-import-save">Load game (file)</button>
    <button id="btn-help">Help / Tutorial</button>
    <p class="hint">Shortcuts: Space pause · +/− speed · 1-5 tabs · D downlink · M map · ? help</p>
  `);

  if (s.unlocked_tech.length) {
    parts.push(`<div class="field"><label>Unlocked R&amp;D</label><div class="chip-row">
      ${s.unlocked_tech
        .map((id) => {
          const t = content.tech_tree.find((n) => n.id === id);
          return `<span class="chip">${esc(t?.name ?? id)}</span>`;
        })
        .join("")}
    </div></div>`);
  }

  parts.push(`<div class="field"><label>Research tree</label></div>`);
  parts.push(`<div class="tech-tree">`);
  for (const tech of content.tech_tree) {
    const owned = s.unlocked_tech.includes(tech.id);
    const locked = tech.requires.some((r) => !s.unlocked_tech.includes(r));
    const state = owned ? "owned" : locked ? "locked" : "available";
    parts.push(`
      <div class="tech-node ${state}">
        <div class="tech-name">${esc(tech.name)}</div>
        <div class="tech-desc">${esc(tech.description)}</div>
        ${owned ? `<span class="chip owned">Owned</span>` : `<button class="btn-tech" data-tech="${esc(tech.id)}" ${locked || s.cash_usd < tech.cost_usd ? "disabled" : ""}>$${tech.cost_usd.toLocaleString()}</button>`}
        ${locked && !owned ? `<span class="tech-req">Requires: ${esc(tech.requires.join(", "))}</span>` : ""}
      </div>
    `);
  }
  parts.push(`</div>`);
  return parts;
}
