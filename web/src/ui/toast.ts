/**
 * Stacked toast notifications with dedupe.
 * Replaces the single-toast overwrite pattern so contested/claim/pass
 * messages don't erase each other.
 */

export type ToastKind = "info" | "success" | "error";

export interface ToastItem {
  id: number;
  message: string;
  kind: ToastKind;
  expiresAt: number;
}

const DEFAULT_TTL_MS = 4200;
const MAX_VISIBLE = 4;
const DEDUPE_MS = 1500;

export class ToastQueue {
  private readonly host: HTMLElement;
  private items: ToastItem[] = [];
  private nextId = 1;
  private lastKey = "";
  private lastAt = 0;
  private raf = 0;

  constructor(host: HTMLElement) {
    this.host = host;
    this.host.classList.add("toast-stack");
    this.host.setAttribute("role", "status");
    this.host.setAttribute("aria-live", "assertive");
  }

  show(message: string, kind: ToastKind = "info", ttlMs = DEFAULT_TTL_MS): void {
    const key = `${kind}:${message}`;
    const now = performance.now();
    if (key === this.lastKey && now - this.lastAt < DEDUPE_MS) return;
    this.lastKey = key;
    this.lastAt = now;

    this.items.push({
      id: this.nextId++,
      message,
      kind,
      expiresAt: now + ttlMs,
    });
    if (this.items.length > MAX_VISIBLE) {
      this.items = this.items.slice(-MAX_VISIBLE);
    }
    this.render();
    this.scheduleTick();
  }

  clear(): void {
    this.items = [];
    this.render();
  }

  private scheduleTick(): void {
    if (this.raf) return;
    const tick = () => {
      this.raf = 0;
      const now = performance.now();
      const before = this.items.length;
      this.items = this.items.filter((t) => t.expiresAt > now);
      if (this.items.length !== before) this.render();
      if (this.items.length > 0) {
        this.raf = window.setTimeout(tick, 200);
      }
    };
    this.raf = window.setTimeout(tick, 200);
  }

  private render(): void {
    if (this.items.length === 0) {
      this.host.innerHTML = "";
      this.host.classList.remove("visible");
      return;
    }
    this.host.classList.add("visible");
    this.host.innerHTML = this.items
      .map(
        (t) =>
          `<div class="toast-item ${t.kind}" data-toast-id="${t.id}">${escapeText(t.message)}</div>`,
      )
      .join("");
  }
}

function escapeText(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
