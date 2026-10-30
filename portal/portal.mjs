// portal.mjs — interactive Discord developer-portal helper for discord-cli.
//
// Drives a real, visible Chromium window via Playwright: the human stays in
// control (any captcha/MFA is solved in Discord's own UI) and the script
// automates only the user's own application management. The user session
// token is captured in-process from the portal's API traffic and is never
// written to disk or stdout; the browser profile persists under the given
// profile dir like any normal browser session.
//
// Auto-login is lazy: credentials are read from 1Password (`op`) ONLY if the
// login form actually appears, so an already-valid session never prompts.
// The op item NAME arrives via env; the secrets it returns stay in memory
// (op stdout), never on argv.
//
// Protocol: exactly one JSON document on stdout at the end; every log line
// goes to stderr. Exit codes match the CLI family: 2 usage, 3 auth (login
// timeout), 5 upstream.

import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';

const PORTAL = 'https://discord.com/developers/applications';
const API = 'https://discord.com/api/v9';
const LOGIN_TIMEOUT_MS = 300_000;
const CREATE_TIMEOUT_MS = 300_000; // room for the human to solve the captcha
const RESET_TIMEOUT_MS = 300_000;

const log = (m) => console.error(`portal: ${m}`);

const [op, ...args] = process.argv.slice(2);
const profileDir = process.env.DISCORD_CLI_PORTAL_PROFILE;
const opItem = process.env.DISCORD_CLI_PORTAL_OP_ITEM || null;
if (!profileDir) {
  log('DISCORD_CLI_PORTAL_PROFILE is not set');
  process.exit(2);
}
if (!['list', 'create', 'reset-token'].includes(op)) {
  log(`unknown operation \`${op}\``);
  process.exit(2);
}

const context = await chromium.launchPersistentContext(profileDir, {
  headless: false,
  viewport: null,
});

let exitCode = 0;
try {
  const out = await run(context, op, args);
  console.log(JSON.stringify(out));
} catch (e) {
  log(e.message);
  exitCode = e.exitCode ?? 5;
} finally {
  await context.close().catch(() => {});
}
process.exit(exitCode);

function fail(exitCode, message) {
  const e = new Error(message);
  e.exitCode = exitCode;
  return e;
}

// --- 1Password (op) — read lazily, in-process, never onto argv ------------
function opField(item, field) {
  return execFileSync('op', ['item', 'get', item, '--fields', `label=${field}`, '--reveal'], {
    encoding: 'utf8',
  }).trim();
}
function opTotp(item) {
  try {
    return execFileSync('op', ['item', 'get', item, '--otp'], { encoding: 'utf8' }).trim() || null;
  } catch {
    return null;
  }
}

async function run(context, op, args) {
  const page = context.pages()[0] ?? (await context.newPage());

  // Resolve the portal Authorization header from the page's own API traffic;
  // it stays in this process only.
  let resolveAuth;
  const authCaptured = new Promise((r) => (resolveAuth = r));
  context.on('request', (req) => {
    const h = req.headers()['authorization'];
    if (h && !h.startsWith('Bot ') && req.url().startsWith('https://discord.com/api/')) {
      resolveAuth(h);
    }
  });

  await page.goto(PORTAL, { waitUntil: 'domcontentloaded' });
  autoLogin(page).catch(() => {}); // best-effort, unawaited — human is the fallback
  log('waiting for the developer portal — log in in the browser window if asked…');
  const auth = await Promise.race([
    authCaptured,
    new Promise((_, rej) =>
      setTimeout(() => rej(fail(3, 'timed out waiting for a Discord login')), LOGIN_TIMEOUT_MS),
    ),
  ]);
  log('portal session ready');
  const headers = { Authorization: auth, 'Content-Type': 'application/json' };

  if (op === 'list') {
    const resp = await context.request.get(`${API}/applications?with_team_applications=true`, {
      headers,
    });
    return apiJson(resp);
  }

  if (op === 'create') {
    const name = args[0];
    if (!name) throw fail(2, 'create needs an application name');
    // Discord now gates app creation behind a ToS checkbox + hCaptcha, so the
    // direct API POST 400s; drive the dialog and let the human clear the
    // captcha in the window.
    return createViaUi(page, name);
  }

  // reset-token: click through the Bot page; MFA password is auto-filled from
  // op when available, else the human completes it. We only read the reset
  // network response for the token.
  const appId = args[0];
  if (!appId) throw fail(2, 'reset-token needs an application ID');
  const resetResp = page.waitForResponse(
    (r) => r.url().includes(`/applications/${appId}/bot/reset`) && r.status() < 300,
    { timeout: RESET_TIMEOUT_MS },
  );
  await page.goto(`${PORTAL}/${appId}/bot`, { waitUntil: 'domcontentloaded' });
  resetFlow(page).catch(() => {}); // best-effort, unawaited — human is the fallback
  log('waiting for the token reset — complete any password/MFA prompt in the window…');
  const body = await resetResp
    .then((r) => r.json())
    .catch(() => {
      throw fail(5, 'timed out waiting for the token reset');
    });
  if (!body.token) throw fail(5, 'reset response contained no token');
  return { token: body.token };
}

async function apiJson(resp) {
  if (!resp.ok()) {
    throw fail(5, `portal API returned HTTP ${resp.status()}: ${await resp.text()}`);
  }
  return resp.json();
}

async function createViaUi(page, name) {
  const created = page.waitForResponse(
    (r) =>
      r.url().endsWith('/api/v9/applications') &&
      r.request().method() === 'POST' &&
      r.status() < 300,
    { timeout: CREATE_TIMEOUT_MS },
  );
  await page.goto(PORTAL, { waitUntil: 'domcontentloaded' });
  try {
    await page.getByRole('button', { name: /new application/i }).first().click({ timeout: 20_000 });
    await page.locator('input[name="name"]').first().fill(name);
    await tickTos(page);
    // The dialog's Create is the last "Create" (the nav "+ Create" is first).
    await page.getByRole('button', { name: /^create$/i }).last().click({ timeout: 15_000 });
    log('if an "Are you human?" captcha appears, solve it in the window to finish');
  } catch (e) {
    log(`could not drive the create dialog (${e.message}) — create “${name}” in the window`);
  }
  return created
    .then((r) => r.json())
    .catch(() => {
      throw fail(5, 'timed out waiting for the application to be created');
    });
}

// Tick the "agree to Developer ToS" checkbox. It's a react-aria widget whose
// real <input> is covered by a visual indicator that intercepts clicks, so
// click the indicator; fall back to force-clicking the input.
async function tickTos(page) {
  const box = page.locator('input[type="checkbox"]').first();
  await page
    .locator('[class*="checkboxIndicator"]')
    .first()
    .click({ timeout: 10_000 })
    .catch(() => {});
  if (!(await box.isChecked().catch(() => false))) {
    await box.click({ force: true }).catch(() => {});
  }
}

async function resetFlow(page) {
  try {
    await page.getByRole('button', { name: /reset token/i }).first().click({ timeout: 20_000 });
    await page.getByRole('button', { name: /yes, do it/i }).first().click({ timeout: 15_000 });
    // Discord re-auths the token reset with the account password.
    const pw = page.locator('input[type="password"]').first();
    const needsPassword = await pw
      .waitFor({ state: 'visible', timeout: 8_000 })
      .then(() => true)
      .catch(() => false);
    if (needsPassword) {
      if (opItem) {
        pw.fill(opField(opItem, 'password'));
        await page.getByRole('button', { name: /^submit$/i }).first().click({ timeout: 10_000 });
      } else {
        log('enter your password in the window to confirm the token reset');
      }
    }
  } catch (e) {
    log(`could not drive the reset (${e.message}) — press “Reset Token” in the window`);
  }
}

// Fill Discord's login form from op, lazily. Handles the "Welcome to the
// Developer Portal" modal (a "Log In" button) that precedes the form. Any
// surprise — captcha, new-location check, redesign, op unavailable — leaves
// the login to the human; the auth capture in run() is the real gate.
async function autoLogin(page) {
  const email = page.locator('input[name="email"]').first();
  const welcomeLogin = page.getByRole('button', { name: /^log in$/i }).first();
  try {
    await Promise.race([
      email.waitFor({ state: 'visible', timeout: 20_000 }),
      welcomeLogin.waitFor({ state: 'visible', timeout: 20_000 }),
    ]);
  } catch {
    log('no login prompt — assuming the session is already valid');
    return;
  }
  // Dismiss the welcome modal to reach the actual form.
  if (!(await email.isVisible().catch(() => false))) {
    await welcomeLogin.click().catch(() => {});
    await email.waitFor({ state: 'visible', timeout: 20_000 }).catch(() => {});
  }
  if (!(await email.isVisible().catch(() => false))) return;

  if (!opItem) {
    log('log in in the browser window (set config `op-item` for auto-login)');
    return;
  }
  log(`auto-filling the Discord login from 1Password item \`${opItem}\`…`);
  try {
    await email.fill(opField(opItem, 'username'));
    await page.fill('input[name="password"]', opField(opItem, 'password'));
    await page.click('button[type="submit"]');
    const totp = opTotp(opItem);
    if (totp) {
      await page
        .locator('input[autocomplete="one-time-code"], input[placeholder*="6-digit"]')
        .first()
        .fill(totp, { timeout: 20_000 })
        .catch(() => {});
      await page.keyboard.press('Enter').catch(() => {});
    }
    log('login submitted — solve any captcha/verification in the window if shown');
  } catch (e) {
    log(`auto-login could not complete (${e.message}) — finish logging in in the window`);
  }
}
