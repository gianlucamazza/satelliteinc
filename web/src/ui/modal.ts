/** In-game confirm dialog reusing the onboarding-card styling. */
export function confirmModal(
  title: string,
  body: string,
  confirmLabel = "Confirm",
): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "onboarding";
    overlay.setAttribute("role", "alertdialog");
    overlay.innerHTML = `
      <div class="onboarding-card">
        <h2></h2>
        <p></p>
        <div class="onboarding-actions">
          <button type="button" class="ghost" data-act="cancel">Cancel</button>
          <button type="button" class="primary" data-act="confirm"></button>
        </div>
      </div>
    `;
    overlay.querySelector("h2")!.textContent = title;
    overlay.querySelector("p")!.textContent = body;
    const confirmBtn = overlay.querySelector<HTMLButtonElement>(
      '[data-act="confirm"]',
    )!;
    confirmBtn.textContent = confirmLabel;

    const done = (result: boolean) => {
      document.removeEventListener("keydown", onKey);
      overlay.remove();
      resolve(result);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") done(false);
      if (e.key === "Enter") done(true);
    };

    confirmBtn.addEventListener("click", () => { done(true); });
    overlay
      .querySelector('[data-act="cancel"]')!
      .addEventListener("click", () => { done(false); });
    document.addEventListener("keydown", onKey);
    document.body.appendChild(overlay);
    confirmBtn.focus();
  });
}
