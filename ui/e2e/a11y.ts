// What every browser test asks of the page: no accessibility violation (axe) and no policy it broke (CSP).
// Shared by the specs, so that a new spec cannot forget the two.

import AxeBuilder from '@axe-core/playwright';
import { expect, type Page } from '@playwright/test';

/** Everything the page reports about a policy it broke, from the start. */
export async function watchViolations(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as unknown as { __violations: string[] };
    w.__violations = [];
    document.addEventListener('securitypolicyviolation', (e) => {
      w.__violations.push(`${e.violatedDirective} ${e.blockedURI}`);
    });
  });
}

export async function violations(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __violations: string[] }).__violations);
}

/** No accessibility violation on the page as it is now (axe). */
export async function expectAccessible(page: Page, what: string): Promise<void> {
  const result = await new AxeBuilder({ page }).analyze();
  expect(result.violations, `${what}: ${JSON.stringify(result.violations, null, 1)}`).toEqual([]);
}
