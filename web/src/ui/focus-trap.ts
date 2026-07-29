const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

let activeRoot: HTMLElement | null = null;
let releaseListener: (() => void) | null = null;
let restoreEl: HTMLElement | null = null;

/** Trap Tab focus inside `root`. Returns a release function. */
export function trapFocus(root: HTMLElement): () => void {
  releaseFocus();
  activeRoot = root;
  restoreEl = document.activeElement as HTMLElement | null;

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const nodes = Array.from(
      root.querySelectorAll<HTMLElement>(FOCUSABLE),
    ).filter((el) => el.offsetParent !== null || el === document.activeElement);
    if (nodes.length === 0) return;
    const first = nodes[0]!;
    const last = nodes[nodes.length - 1]!;
    if (e.shiftKey) {
      if (document.activeElement === first || !root.contains(document.activeElement)) {
        e.preventDefault();
        last.focus();
      }
    } else if (document.activeElement === last || !root.contains(document.activeElement)) {
      e.preventDefault();
      first.focus();
    }
  };

  root.addEventListener("keydown", onKeyDown);
  releaseListener = () => {
    root.removeEventListener("keydown", onKeyDown);
    if (activeRoot === root) activeRoot = null;
    releaseListener = null;
  };

  const first = root.querySelector<HTMLElement>(FOCUSABLE);
  first?.focus();

  return releaseListener;
}

/** End the active trap and optionally restore focus. */
export function releaseFocus(restore?: HTMLElement | null): void {
  releaseListener?.();
  const el = restore ?? restoreEl;
  restoreEl = null;
  activeRoot = null;
  if (el?.isConnected) el.focus();
}

/** True when a modal/dialog has trapped focus or is visibly open. */
export function isModalOpen(): boolean {
  if (activeRoot?.isConnected) return true;
  return !!document.querySelector(
    ".onboarding:not(.hidden), #endscreen:not(.hidden)",
  );
}