import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { expectAccessible, violations, watchViolations } from './a11y';
import { STATE } from './global';
import { HOSTILE, ORIGIN, pairingLink } from './hub';

test.use({ locale: 'en-US' });

const state = (): { config: string; vault: string; token: string } =>
  JSON.parse(readFileSync(STATE, 'utf8')) as { config: string; vault: string; token: string };

/** What the page wrote to the console: an error or a warning is a finding. */
const consoleProblems = new WeakMap<Page, string[]>();

async function prepare(page: Page): Promise<void> {
  const problems: string[] = [];
  consoleProblems.set(page, problems);
  page.on('console', (m) => {
    if (m.type() === 'error' || m.type() === 'warning') problems.push(`${m.type()}: ${m.text()}`);
  });
  page.on('pageerror', (e) => problems.push(`pageerror: ${e.message}`));
  await watchViolations(page);
}

/** The tab holds the session, as it does after the link was followed. */
async function pair(page: Page): Promise<void> {
  await prepare(page);
  await page.addInitScript((token) => {
    sessionStorage.setItem('rules-desktop.session', token);
  }, state().token);
  await page.goto('/');
  await expect(page.getByRole('menubar')).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: 'live' })).toBeVisible();
}

async function view(page: Page, name: 'Overview' | 'Families' | 'Rules'): Promise<void> {
  await page.getByRole('navigation').getByRole('button', { name, exact: true }).click();
}

async function noProblems(page: Page): Promise<void> {
  // A test that never prepared the page (it only asks the hub) has nothing to report.
  if (!consoleProblems.has(page)) return;
  expect(await violations(page), 'a policy was broken').toEqual([]);
  expect(consoleProblems.get(page) ?? [], 'the console had findings').toEqual([]);
}

test.afterEach(async ({ page }) => {
  await noProblems(page);
});

test('the link pairs the tab, leaves no code in the address and sets no cookie', async ({ page, context }) => {
  await prepare(page);
  await page.goto(pairingLink(state().config));
  await expect(page.getByRole('menubar')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible();
  expect(page.url()).toBe(`${ORIGIN}/`);
  expect(await page.evaluate(() => sessionStorage.getItem('rules-desktop.session'))).toMatch(/^[0-9a-f]{64}$/);
  expect(await context.cookies()).toEqual([]);
  // Nothing of a rule set stays in the browser: only the session, per tab.
  expect(await page.evaluate(() => Object.keys(localStorage))).toEqual([]);
});

test('a tab without a session is told how to pair and is shown no rule', async ({ page }) => {
  await prepare(page);
  await page.goto('/');
  await expect(page.getByText('This tab is not paired with the hub.')).toBeVisible();
  await expect(page.getByRole('navigation')).toHaveCount(0);
  await expectAccessible(page, 'not paired');
});

test('the data routes need a session and the page carries a strict policy', async ({ request }) => {
  for (const path of ['/api/rules/snapshot', '/api/rules/sources', '/api/rules/search?q=text', '/api/rules/events']) {
    expect((await request.get(path)).status(), path).toBe(401);
  }
  const page = await request.get('/');
  const csp = page.headers()['content-security-policy'] ?? '';
  expect(csp).toContain("default-src 'none'");
  expect(csp).toContain("script-src 'self'");
  expect(csp).not.toContain('unsafe');
  expect(page.headers()['x-content-type-options']).toBe('nosniff');
  // A foreign origin is refused whatever the credential.
  const foreign = await request.get('/api/rules/snapshot', {
    headers: { Origin: 'https://evil.example', Authorization: `Bearer ${state().token}` },
  });
  expect(foreign.status()).toBe(403);
});

test('the three views and their counters', async ({ page }) => {
  await pair(page);
  await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible();
  // secret has no class on this device: that is a finding, and the page says so.
  await expect(page.getByText('Files without a class on this device (not released): 1')).toBeVisible();
  await view(page, 'Families');
  const table = page.getByRole('table', { name: 'Families' });
  await expect(table.getByRole('row', { name: /^AA / })).toBeVisible();
  await expect(table.getByRole('row', { name: /^BB / })).toBeVisible();
  await view(page, 'Rules');
  await expect(page.locator('tr#rule-AA-01')).toBeVisible();
  // A row of a family opens the list narrowed to it.
  await view(page, 'Families');
  await page.getByRole('button', { name: 'BB', exact: true }).click();
  await expect(page.locator('tr#rule-BB-01')).toBeVisible();
  await expect(page.locator('tr#rule-AA-01')).toHaveCount(0);
});

test('the class of a rule is the one of its file on this device', async ({ page }) => {
  await pair(page);
  await view(page, 'Rules');
  await expect(page.locator('tr#rule-AA-01')).toContainText('public');
  await expect(page.locator('tr#rule-BB-01')).toContainText('internal');
  // `secret` has no entry in sources.json: shown, and marked as not classified.
  await expect(page.locator('tr#rule-SS-01')).toContainText('not classified');
});

test('the keyboard: chord, search, palette and escape', async ({ page }) => {
  await pair(page);
  await page.keyboard.press('g');
  await page.keyboard.press('f');
  await expect(page.getByRole('table', { name: 'Families' })).toBeVisible();
  await page.keyboard.press('g');
  await page.keyboard.press('l');
  await expect(page.locator('tr#rule-AA-01')).toBeVisible();
  await page.keyboard.press('/');
  await expect(page.getByRole('searchbox')).toBeFocused();
  // A letter typed into the box is text, not a shortcut.
  await page.keyboard.type('gl');
  await expect(page.getByRole('searchbox')).toHaveValue('gl');
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('the search reads the texts of the rules', async ({ page }) => {
  await pair(page);
  await view(page, 'Rules');
  await page.getByRole('searchbox').fill('never dropped');
  // The hub reads the texts a moment after it starts: the list narrows when they are in.
  await expect(page.locator('tr#rule-AA-01')).toBeVisible({ timeout: 20_000 });
  await expect(page.locator('tr#rule-AA-02')).toHaveCount(0);
  await expect(page.getByText(/1 of \d+/)).toBeVisible();
});

test('a rule is shown as text, with its file and its check as data', async ({ page }) => {
  await pair(page);
  await view(page, 'Rules');
  await page.locator('tr#rule-AA-01').getByRole('button').click();
  const detail = page.getByRole('complementary', { name: 'Rule' });
  await expect(detail.getByRole('heading', { name: 'AA-01' })).toBeVisible();
  // The markup in the text is what is read, character for character.
  await expect(detail).toContainText('<b>Not markup</b>');
  expect(await detail.locator('b').count()).toBe(0);
  await expect(detail).toContainText('"type": "grep"');
  await expect(detail).toContainText('alpha/regeln/alpha.md:');
  await page.keyboard.press('Escape');
  await expect(detail).toHaveCount(0);
});

test('a hostile rule set is text, never markup, never a script', async ({ page }) => {
  await pair(page);
  await view(page, 'Rules');
  await expect(page.locator('tr#rule-HX-01')).toContainText('<img src=x onerror=window.__xss=1>');
  await expect(page.locator('tr#rule-HX-02')).toContainText('<svg onload=window.__xss=2>');
  for (const h of HOSTILE) {
    await page.locator(`tr#rule-${h.id}`).getByRole('button').click();
    const detail = page.getByRole('complementary', { name: 'Rule' });
    await expect(detail.getByRole('heading', { name: h.id })).toBeVisible();
    // No element came out of the text.
    expect(await detail.locator('script, img, iframe, svg, details, form, style').count(), h.id).toBe(0);
    expect(await detail.locator('a[href]').count(), h.id).toBe(0);
  }
  expect(await page.evaluate(() => (window as unknown as { __xss?: unknown }).__xss)).toBeUndefined();
  // The hidden characters of a title are made visible, in the list and in the detail.
  await page.locator('tr#rule-HX-06').getByRole('button').click();
  await expect(page.getByRole('complementary', { name: 'Rule' })).toContainText('⟦U+202E⟧');
  await expect(page.getByRole('complementary', { name: 'Rule' })).toContainText('The text holds hidden characters');
  await expectAccessible(page, 'a hostile rule open');
});

test('a rule file that appears on disk appears on the page by itself', async ({ page }) => {
  await pair(page);
  await view(page, 'Rules');
  await expect(page.locator('tr#rule-AA-01')).toBeVisible();
  // The engine does not trust a file younger than two seconds.
  await page.waitForTimeout(2300);
  writeFileSync(
    join(state().vault, 'rulesets', 'alpha', 'regeln', 'live.md'),
    '# Live\n\n#### `LV-01` - Written while the page was open\n\n```rule\nid = "LV-01",\nseverity = "critical",\n```\n\nA live rule.\n',
  );
  await expect(page.locator('tr#rule-LV-01')).toBeVisible({ timeout: 20_000 });
  // The search follows the change, too.
  await page.getByRole('searchbox').fill('live rule');
  await expect(page.locator('tr#rule-LV-01')).toBeVisible({ timeout: 20_000 });
});

test('German, the dark theme and the palette', async ({ page }) => {
  await pair(page);
  await page.getByRole('menuitem', { name: 'Language', exact: true }).click();
  await page.getByRole('menuitemradio', { name: 'Deutsch', exact: true }).click();
  await expect(page.getByRole('navigation').getByRole('button', { name: 'Regeln', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.lang)).toBe('de');
  await page.getByRole('menuitem', { name: 'Farben', exact: true }).click();
  await page.getByRole('menuitemradio', { name: 'dunkel', exact: true }).click();
  expect(await page.evaluate(() => document.documentElement.dataset['theme'])).toBe('dark');
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('dialog', { name: 'Befehlspalette' })).toBeVisible();
  await expectAccessible(page, 'palette, dark, German');
});

for (const scheme of ['light', 'dark'] as const) {
  test(`no accessibility violation in ${scheme}: every view and the detail`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await pair(page);
    await expectAccessible(page, `overview ${scheme}`);
    await view(page, 'Families');
    await expectAccessible(page, `families ${scheme}`);
    await view(page, 'Rules');
    await expect(page.locator('tr#rule-AA-01')).toBeVisible();
    await expectAccessible(page, `rules ${scheme}`);
    await page.locator('tr#rule-AA-01').getByRole('button').click();
    await expect(page.getByRole('complementary', { name: 'Rule' })).toBeVisible();
    await expectAccessible(page, `detail ${scheme}`);
    await page.keyboard.press('Control+k');
    await expect(page.getByRole('dialog')).toBeVisible();
    await expectAccessible(page, `palette ${scheme}`);
  });
}

test('Back and Forward walk the views and the open rule, and the address stays clean', async ({ page }) => {
  await pair(page);
  const detail = page.getByRole('complementary', { name: 'Rule' });
  await view(page, 'Families');
  await view(page, 'Rules');
  await page.locator('tr#rule-AA-01').getByRole('button').click();
  await expect(detail.getByRole('heading', { name: 'AA-01' })).toBeVisible();
  // The place is in the history of the tab, not in the address: no rule id, no fragment.
  expect(page.url()).toBe(`${ORIGIN}/`);
  await page.goBack();
  await expect(detail).toHaveCount(0);
  await expect(page.locator('tr#rule-AA-01')).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('table', { name: 'Families' })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible();
  await page.goForward();
  await page.goForward();
  await page.goForward();
  await expect(detail.getByRole('heading', { name: 'AA-01' })).toBeVisible();
  expect(page.url()).toBe(`${ORIGIN}/`);
});
