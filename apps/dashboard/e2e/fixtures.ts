import { test as base, expect, type Page } from '@playwright/test';
import { ADMIN_EMAIL, ADMIN_PASSWORD } from './constants';

export * from './constants';

export function uniqueName(prefix: string): string {
	return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 10000)}`;
}

export async function loginViaUi(
	page: Page,
	email = ADMIN_EMAIL,
	password = ADMIN_PASSWORD
): Promise<void> {
	await page.goto('/login');
	await page.getByLabel('Email Address').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign In' }).click();
	await page.waitForURL(/\/dashboard/);
}

export async function loginViaApi(
	page: Page,
	email = ADMIN_EMAIL,
	password = ADMIN_PASSWORD
): Promise<void> {
	const res = await page.request.post('/v1/auth/login', {
		data: { email, password },
	});
	expect(res.ok(), `API login failed with status ${res.status()}`).toBeTruthy();
}

export const test = base.extend<{ authedPage: Page }>({
	authedPage: async ({ page }, use) => {
		await loginViaApi(page);
		await use(page);
	},
});

export { expect };
