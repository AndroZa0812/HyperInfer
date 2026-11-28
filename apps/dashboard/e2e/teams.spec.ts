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

		// Edit team name & budget (HYP-58)
		const editedName = `${teamName}-Edited`;
		await page.getByRole('button', { name: /Edit Team/ }).click();
		const editDialog = page.getByRole('dialog');
		await expect(editDialog.getByRole('heading', { name: 'Edit Team' })).toBeVisible();
		await editDialog.getByLabel('Team Name').fill(editedName);
		await editDialog.getByLabel('Budget (cents)').fill('50000');
		await editDialog.getByRole('button', { name: 'Save Team' }).click();
		await expect(editDialog).toBeHidden();
		await expect(page.getByRole('heading', { name: editedName })).toBeVisible();
		await expect(page.getByText('$500.00').first()).toBeVisible();

		// Configure Rate-Limit Quotas (HYP-56)
		await page.getByRole('button', { name: 'Configure Quotas' }).click();
		const quotaDialog = page.getByRole('dialog');
		await expect(quotaDialog.getByRole('heading', { name: 'Configure Rate-Limit Quotas' })).toBeVisible();
		await quotaDialog.getByLabel('RPM Limit').fill('120');
		await quotaDialog.getByLabel('TPM Limit').fill('250000');
		await quotaDialog.getByRole('button', { name: 'Save Quotas' }).click();
		await expect(quotaDialog).toBeHidden();
		await expect(page.getByText('120', { exact: true })).toBeVisible();
		await expect(page.getByText('250,000', { exact: true })).toBeVisible();

		// Add & delete Model Alias (HYP-56)
		await page.getByRole('button', { name: /Add Alias/ }).click();
		const aliasDialog = page.getByRole('dialog');
		await expect(aliasDialog.getByRole('heading', { name: 'Add Model Alias' })).toBeVisible();
		await aliasDialog.getByLabel('Alias').fill('fast-chat');
		await aliasDialog.getByLabel('Target Model').fill('gpt-4o-mini');
		await aliasDialog.getByLabel('Provider').click();
		await page.getByRole('option', { name: 'OpenAI' }).click();
		await aliasDialog.getByRole('button', { name: 'Create Alias' }).click();
		await expect(aliasDialog).toBeHidden();

		const aliasRow = page.locator('tbody tr', { hasText: 'fast-chat' });
		await expect(aliasRow).toBeVisible();
		await expect(aliasRow).toContainText('gpt-4o-mini');
		await aliasRow.getByRole('button', { name: 'Delete' }).click();
		await expect(page.locator('tbody tr', { hasText: 'fast-chat' })).toHaveCount(0);

		const teamUrl = page.url();

		// Verify team Usage sub-route loads without runtime crash
		await page.getByRole('link', { name: /Usage/ }).click();
		await expect(page).toHaveURL(/\/dashboard\/teams\/[^/]+\/usage$/);
		await expect(page.getByRole('heading', { name: 'Usage Analytics' })).toBeVisible();
		await expect(page.getByText('No time-series usage data recorded yet')).toBeVisible();

		// Verify team Budget sub-route loads without runtime crash
		await page.goto(`${teamUrl}/budget`);
		await expect(page.getByRole('heading', { name: `Budget - ${editedName}` })).toBeVisible();
		await expect(page.getByText('$500.00')).toBeVisible();
		await expect(page.getByText('$$500.00')).toHaveCount(0);

		// Navigate to team-scoped API keys page
		await page.goto(`${teamUrl}/keys`);
		await expect(page.getByRole('heading', { name: 'API Keys' })).toBeVisible();
		await expect(page.getByText('No API keys yet')).toBeVisible();

		// Delete team and verify redirect + removal (HYP-58)
		await page.goto(teamUrl);
		await page.getByRole('button', { name: /Delete Team/ }).click();
		const deleteDialog = page.getByRole('dialog');
		await expect(deleteDialog.getByRole('heading', { name: 'Delete Team' })).toBeVisible();
		await deleteDialog.getByRole('button', { name: 'Delete', exact: true }).click();
		await expect(page).toHaveURL(/\/dashboard\/teams$/);
		await expect(page.locator('tbody tr', { hasText: editedName })).toHaveCount(0);

		expect(pageErrors).toEqual([]);
	});
});
