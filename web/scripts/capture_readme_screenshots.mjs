/**
 * Axonel README Screenshot Generator
 *
 * Runs the real Axonel server, launches Playwright Chromium, drives the full
 * autonomous mission lifecycle, and captures focused, high-resolution screenshots
 * for the README and documentation.
 */

import { spawn, execFileSync } from "child_process";
import fs from "fs";
import path from "path";
import { chromium } from "@playwright/test";

const PORT = 4088;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const DB_PATH = `/tmp/axonel_screenshots_${Date.now()}.db`;
const projectRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const releaseBinary = path.join(projectRoot, "target/release/axonel");
const debugBinary = path.join(projectRoot, "target/debug/axonel");
const binToUse = fs.existsSync(releaseBinary) ? releaseBinary : debugBinary;

const outDirHero = path.join(projectRoot, "docs/readme-assets/hero");
const outDirScreens = path.join(projectRoot, "docs/readme-assets/screenshots");
fs.mkdirSync(outDirHero, { recursive: true });
fs.mkdirSync(outDirScreens, { recursive: true });

console.log("========================================================================");
console.log("   AXONEL README SCREENSHOT GENERATOR");
console.log(`   Binary in use: ${binToUse}`);
console.log("========================================================================\n");

let serverProc = null;
const tempDirs = [];

function createTargetRepo() {
  const dir = `/tmp/axonel_screens_repo_${Date.now()}`;
  fs.mkdirSync(dir, { recursive: true });
  tempDirs.push(dir);

  execFileSync("git", ["init", "-b", "main"], { cwd: dir });
  execFileSync("git", ["config", "user.name", "Axonel Screenshot Agent"], { cwd: dir });
  execFileSync("git", ["config", "user.email", "agent@axonel.local"], { cwd: dir });

  fs.mkdirSync(path.join(dir, "src"), { recursive: true });
  fs.writeFileSync(
    path.join(dir, "Cargo.toml"),
    `[package]\nname = "math_demo"\nversion = "0.1.0"\nedition = "2021"\n`
  );
  fs.writeFileSync(
    path.join(dir, "src/lib.rs"),
    `pub fn add(a: i32, b: i32) -> i32 {\n    a - b // BUG: subtraction instead of addition\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_add() {\n        assert_eq!(add(2, 3), 5);\n    }\n}\n`
  );
  fs.writeFileSync(path.join(dir, ".gitignore"), "/target\nCargo.lock\n.plexis/\n", "utf-8");

  execFileSync("git", ["add", "-A"], { cwd: dir });
  execFileSync("git", ["commit", "-m", "initial buggy math implementation"], { cwd: dir });
  return dir;
}

function startServer() {
  console.log(`Starting Axonel server on ${BASE_URL}...`);
  const env = {
    ...process.env,
    PORT: PORT.toString(),
    PLEXIS_DB_PATH: DB_PATH,
    RUST_LOG: "info",
  };
  const args = ["serve", "--host", "127.0.0.1", "--port", PORT.toString(), "--db", DB_PATH];
  const p = spawn(binToUse, args, {
    cwd: projectRoot,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  return p;
}

async function waitForServer(timeoutMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(`${BASE_URL}/health`);
      if (res.ok) return true;
    } catch {}
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`Server failed to start within ${timeoutMs}ms`);
}

async function cleanup() {
  console.log("\n--- Cleaning Up Resources ---");
  if (serverProc) {
    try {
      serverProc.kill("SIGKILL");
    } catch {}
  }
  for (const d of tempDirs) {
    try {
      fs.rmSync(d, { recursive: true, force: true });
    } catch {}
  }
  try {
    if (fs.existsSync(DB_PATH)) fs.unlinkSync(DB_PATH);
  } catch {}
}

process.on("SIGINT", async () => {
  await cleanup();
  process.exit(1);
});

async function run() {
  let browser = null;
  try {
    const targetRepoDir = createTargetRepo();
    serverProc = startServer();
    await waitForServer();
    console.log("✓ Server healthy on loopback.");

    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2, // High-DPI retina screenshots
    });
    const page = await context.newPage();
    page.on("dialog", async (dialog) => {
      console.log(`[Browser Dialog] ${dialog.message()}`);
      await dialog.accept();
    });

    console.log("Navigating to Axonel UI...");
    await page.goto(BASE_URL, { waitUntil: "networkidle" });

    // Register Workspace
    console.log("Registering workspace in UI...");
    await page.click('button[title="Switch project workspace"]');
    await page.waitForSelector('text="Project Workspaces"', { timeout: 5000 });
    const addWsBtn = page.locator('button:has-text("Register Workspace"), button:has-text("Register First Workspace")').first();
    await addWsBtn.click();
    await page.fill('input[placeholder*="token-limiter"]', "Math Demo");
    await page.fill('input[placeholder*="/home/user/Projects"]', targetRepoDir);
    await page.click('button[type="submit"]:has-text("Register Workspace")');
    const doneBtn = page.locator('button:has-text("Done")');
    if (await doneBtn.isVisible()) await doneBtn.click();
    await page.waitForTimeout(1000);

    // Launch Mission with fake_agent
    console.log("Launching autonomous mission...");
    await page.click('button:has-text("New Mission")');
    await page.waitForSelector('text="Launch Autonomous Mission"', { timeout: 5000 });
    await page.fill('input[placeholder*="Long-Horizon"]', "Fix Math Addition Bug");
    await page.fill(
      'textarea[placeholder*="Describe the software engineering objective"]',
      "Fix the subtraction bug in src/lib.rs so add(2, 3) == 5. Verify passing cargo test and commit."
    );
    await page.selectOption('form select:has(option[value="fake_agent"])', "fake_agent");
    await page.click('button[type="submit"]:has-text("Launch Mission")');

    // Wait for mission to reach READY FOR REVIEW
    console.log("Waiting for autonomous execution and physical verification...");
    const readyBadge = page.locator('span:has-text("READY FOR REVIEW")').first();
    await readyBadge.waitFor({ state: "visible", timeout: 30000 });
    await page.waitForTimeout(1000);
    console.log("✓ Mission reached READY FOR REVIEW ('AwaitingAcceptance').");

    // 1. Capture Dashboard Hero Screenshot
    console.log("Capturing hero dashboard screenshot...");
    const heroPath = path.join(outDirHero, "dashboard-hero.png");
    await page.screenshot({ path: heroPath });
    console.log(`✓ Saved hero: ${heroPath}`);

    // 2. Capture Mission Detail & Isolated Worktree Card
    console.log("Capturing isolated worktree detail card...");
    // Find the right-side detail card containing stopping conditions and git provenance
    const detailPanel = page.locator('div:has-text("STOPPING CONDITIONS")').locator("xpath=ancestor::div[contains(@class, 'flex-1') or contains(@class, 'p-6')]").first();
    const worktreeCropPath = path.join(outDirScreens, "01-isolated-worktree.png");
    if (await detailPanel.isVisible()) {
      await detailPanel.screenshot({ path: worktreeCropPath });
    } else {
      await page.screenshot({ path: worktreeCropPath, clip: { x: 380, y: 70, width: 1040, height: 750 } });
    }
    console.log(`✓ Saved worktree crop: ${worktreeCropPath}`);

    // 3. Open Review Package Modal
    console.log("Opening Review Package modal...");
    await page.click('button:has-text("Review")');
    await page.waitForSelector('text="Mission Review Package"', { timeout: 5000 });
    await page.waitForTimeout(1000);

    // Capture Verification Result Card inside Modal
    console.log("Capturing independent verification results...");
    const modal = page.locator('div[role="dialog"], div.fixed:has-text("Mission Review Package")').first();
    const verificationCropPath = path.join(outDirScreens, "02-independent-verification.png");
    await modal.screenshot({ path: verificationCropPath });
    console.log(`✓ Saved review package: ${verificationCropPath}`);

    // 4. Capture Human Acceptance Actions (crop from the banner on the main dashboard)
    console.log("Capturing human acceptance action panel...");
    // Close modal first
    const closeBtn = page.locator('button:has-text("Close"), button[title="Close"]').first();
    if (await closeBtn.isVisible()) {
      await closeBtn.click();
      await page.waitForTimeout(500);
    }

    const reviewBanner = page.locator('div:has-text("Physical Verification Passed — Awaiting Human Acceptance")').locator("xpath=ancestor::div[contains(@class, 'rounded')]").first();
    const acceptanceCropPath = path.join(outDirScreens, "04-human-acceptance.png");
    if (await reviewBanner.isVisible()) {
      await reviewBanner.screenshot({ path: acceptanceCropPath });
      console.log(`✓ Saved acceptance banner: ${acceptanceCropPath}`);
    }

    // 5. Click Accept & Integrate on the dashboard banner
    console.log("Clicking 'Accept & Integrate'...");
    const acceptIntegrateBtn = page.locator('button:has-text("Accept & Integrate")').first();
    await acceptIntegrateBtn.click();
    await page.waitForTimeout(2000);

    const integratedBadge = page.locator('span:has-text("INTEGRATED")').first();
    await integratedBadge.waitFor({ state: "visible", timeout: 10000 });
    console.log("✓ Mission transitioned to INTEGRATED.");

    const integrationCropPath = path.join(outDirScreens, "05-git-integration.png");
    await page.screenshot({ path: integrationCropPath, clip: { x: 380, y: 70, width: 1040, height: 420 } });
    console.log(`✓ Saved integration crop: ${integrationCropPath}`);

    console.log("\n========================================================================");
    console.log("   ALL SCREENSHOTS CAPTURED SUCCESSFULLY!");
    console.log("========================================================================\n");
  } finally {
    if (browser) await browser.close();
    await cleanup();
  }
}

run().catch((err) => {
  console.error("Screenshot capture failed:", err);
  cleanup().then(() => process.exit(1));
});
