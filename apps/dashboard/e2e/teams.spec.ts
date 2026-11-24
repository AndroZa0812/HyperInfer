import { test, expect, SEED_TEAM_NAME, uniqueName } from './fixtures';

test.describe('Teams', () => {
	test('displays the seeded default team', async ({ authedPage: page }) => {
		await page.goto('/dashboard/teams');
		await expect(page.getByRole('heading', { name: 'Teams', exact: true })).toBeVisible();

		const defaultRow = page.locator('tbody tr', { hasText: SEED_TEAM_NAME });
		await expect(defaultRow).toBeVisible();
		await expect(defaultRow).toContainText('$100.00');
	});

	test('creates a new team, persists across reload, and navigates to team detail', async ({
		authedPage: page,
	}) => {
		const teamName = uniqueName('Team');
		await page.goto('/dashboard/teams');

		await page.getByRole('button', { name: /Create Team/ }).click();
		const dialog = page.getByRole('dialog');
		await expect(dialog.getByRole('heading', { name: 'Create Team' })).toBeVisible();

		await dialog.getByPlaceholder('e.g. Engineering Alpha').fill(teamName);
		await dialog.getByPlaceholder('10000').fill('25000');
		await dialog.getByRole('button', { name: 'Create', exact: true }).click();

		await expect(dialog).toBeHidden();

		const teamRow = page.locator('tbody tr', { hasText: teamName });
		await expect(teamRow).toBeVisible();
		await expect(teamRow).toContainText('$250.00');

		// Reload and confirm persistence
		await page.reload();
		const persistedRow = page.locator('tbody tr', { hasText: teamName });
		await expect(persistedRow).toBeVisible();

		const pageErrors: string[] = [];
		page.on('pageerror', (err) => pageErrors.push(err.message));

		// Navigate to team detail page
		await persistedRow.getByRole('link', { name: /View/ }).click();
		await expect(page).toHaveURL(/\/dashboard\/teams\/[^/]+$/);
		await expect(page.getByRole('heading', { name: teamName })).toBeVisible();
		await expect(page.getByRole('heading', { name: 'Monthly Budget' })).toBeVisible();
		await expect(page.getByText('$250.00').first()).toBeVisible();
		await expect(page.getByText('$$250.00')).toHaveCount(0);

		const teamUrl = page.url();

		// Verify team Usage sub-route loads without runtime crash
		await page.getByRole('link', { name: /Usage/ }).click();
		await expect(page).toHaveURL(/\/dashboard\/teams\/[^/]+\/usage$/);
		await expect(page.getByRole('heading', { name: 'Usage Analytics' })).toBeVisible();
		await expect(page.getByText('No time-series usage data recorded yet')).toBeVisible();

		// Verify team Budget sub-route loads without runtime crash
		await page.goto(`${teamUrl}/budget`);
		await expect(page.getByRole('heading', { name: `Budget - ${teamName}` })).toBeVisible();
		await expect(page.getByText('$250.00')).toBeVisible();
		await expect(page.getByText('$$250.00')).toHaveCount(0);

		// Navigate to team-scoped API keys page
		await page.goto(`${teamUrl}/keys`);
		await expect(page.getByRole('heading', { name: 'API Keys' })).toBeVisible();
		await expect(page.getByText('No API keys yet')).toBeVisible();

		expect(pageErrors).toEqual([]);
	});
});
