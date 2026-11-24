import { test, expect, ADMIN_EMAIL, ADMIN_PASSWORD, loginViaUi } from './fixtures';

test.describe('Authentication', () => {
	test('redirects unauthenticated users from / and /dashboard to /login', async ({ page }) => {
		await page.goto('/');
		await expect(page).toHaveURL(/\/login$/);
		await expect(page.getByRole('heading', { name: 'Welcome Back' })).toBeVisible();
		await expect(page.getByText('2027 HyperInfer')).toBeVisible();
		await expect(page.getByRole('button', { name: 'Google' })).toHaveCount(0);
		await expect(page.getByRole('button', { name: 'SSO' })).toHaveCount(0);
		await expect(page.getByRole('button', { name: 'Forgot password?' })).toHaveCount(0);
		await expect(page.getByRole('button', { name: 'Request Access' })).toHaveCount(0);

		await page.goto('/dashboard/keys');
		await expect(page).toHaveURL(/\/login$/);
	});

	test('shows an error for invalid credentials', async ({ page }) => {
		await page.goto('/login');
		await page.getByLabel('Email Address').fill(ADMIN_EMAIL);
		await page.getByLabel('Password').fill('wrong-password-123');
		await page.getByRole('button', { name: 'Sign In' }).click();

		await expect(page.getByText('Invalid credentials')).toBeVisible();
		await expect(page).toHaveURL(/\/login$/);
	});

	test('logs in as admin, persists session across reload, and logs out', async ({ page }) => {
		await loginViaUi(page, ADMIN_EMAIL, ADMIN_PASSWORD);

		// Admin lands on /dashboard/teams via /dashboard redirect
		await expect(page).toHaveURL(/\/dashboard\/teams$/);
		await expect(page.locator('aside').getByText(ADMIN_EMAIL)).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Teams' })).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Keys' })).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Routing' })).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Settings' })).toBeVisible();

		// Session persists after full page reload
		await page.reload();
		await expect(page).toHaveURL(/\/dashboard\/teams$/);
		await expect(page.locator('aside').getByText(ADMIN_EMAIL)).toBeVisible();

		// Logout returns to /login and clears session
		await page.locator('aside').getByRole('button', { name: /Logout/ }).click();
		await expect(page).toHaveURL(/\/login$/);

		await page.goto('/dashboard');
		await expect(page).toHaveURL(/\/login$/);
	});
});
