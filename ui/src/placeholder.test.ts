import { expect, test } from 'vitest';

// Keeps `vitest run` meaningful (it fails with no test file) until real tests arrive.
test('the test runner runs', () => {
  expect(1 + 1).toBe(2);
});
