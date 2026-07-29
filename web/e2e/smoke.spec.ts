import { expect, test } from "@playwright/test";

/**
 * Smoke: WASM + HUD boot. Does not drive the full campaign (too flaky for CI
 * without a seeded sim harness) but catches black screens and load hangs.
 */
test("boots HUD after loading stages", async ({ page }) => {
  await page.goto("/");
  // Loading overlay should eventually hide.
  await expect(page.locator("#loading")).toBeHidden({ timeout: 90_000 });
  await expect(page.locator("#hud")).toBeVisible();
  await expect(page.locator("#hud h1")).toContainText("Satellite Inc");
  // Mission control tabs or status present.
  await expect(page.locator("#tab-bar")).toBeVisible();
  await expect(page.locator("#action-panel")).toBeVisible();
  await expect(page.locator("#hud-topbar")).toBeVisible();
});

test("priority strip renders after first tick", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#loading")).toBeHidden({ timeout: 90_000 });
  // Global telemetry lives in the top bar (outside the left dock).
  await expect(page.locator("#priority-strip")).toBeVisible({ timeout: 15_000 });
  await expect(page.locator("#priority-strip")).toContainText("Cash");
  await expect(page.locator("#phase-label")).toBeVisible();
});
