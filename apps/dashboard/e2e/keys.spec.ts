import { test, expect, uniqueName } from './fixtures';

test.describe('API Keys', () => {
	test('creates, searches, filters, and revokes an API key', async ({ authedPage: page }) => {
		const activeKeyName = uniqueName('KeyActive');
		const revokeKeyName = uniqueName('KeyRevoke');

		await page.goto('/dashboard/keys');
		await expect(page.getByRole('heading', { name: 'API Keys' })).toBeVisible();

		// Create first key with virtual-key budget & allowed models (HYP-68)
		await page.getByRole('button', { name: /Create New Key/ }).click();
		const dialog = page.getByRole('dialog');
		await expect(dialog.getByRole('heading', { name: 'Create API Key' })).toBeVisible();
		await dialog.getByPlaceholder('e.g. Production-Main').fill(activeKeyName);
		await dialog.getByLabel('Budget (cents, optional)').fill('5000');
		await dialog.getByLabel('Allowed Models (comma-separated, optional)').fill('gpt-4, gpt-4o-mini');
		await dialog.getByRole('button', { name: 'Create Key' }).click();

		// One-time secret modal should display the raw hi-... key
		await expect(dialog.getByRole('heading', { name: 'API Key Created' })).toBeVisible();
		await expect(dialog.getByText(/^hi-/)).toBeVisible();
		await dialog.getByRole('button', { name: 'Done' }).click();
		await expect(dialog).toBeHidden();

		const activeRow = page.locator('tbody tr', { hasText: activeKeyName });
		await expect(activeRow).toBeVisible();
		await expect(activeRow.getByText('Active', { exact: true })).toBeVisible();
		await expect(activeRow).toContainText('$50.00');
		await expect(activeRow).toContainText('gpt-4, gpt-4o-mini');
		await expect(activeRow).not.toContainText('undefined');

		// Create second key to revoke
		await page.getByRole('button', { name: /Create New Key/ }).click();
		await dialog.getByPlaceholder('e.g. Production-Main').fill(revokeKeyName);
		await dialog.getByRole('button', { name: 'Create Key' }).click();
		await expect(dialog.getByRole('heading', { name: 'API Key Created' })).toBeVisible();
		await dialog.getByRole('button', { name: 'Done' }).click();
		await expect(dialog).toBeHidden();

		const revokeRow = page.locator('tbody tr', { hasText: revokeKeyName });
		await expect(revokeRow).toBeVisible();

		// Revoke second key
		await revokeRow.getByTitle('Revoke').click();
		await expect(revokeRow.getByText('Revoked', { exact: true })).toBeVisible();
		await expect(revokeRow.getByText('No Actions Available')).toBeVisible();

		// Filter by status tabs
		await page.getByRole('button', { name: 'Revoked', exact: true }).click();
		await expect(page.locator('tbody tr', { hasText: revokeKeyName })).toBeVisible();
		await expect(page.locator('tbody tr', { hasText: activeKeyName })).toHaveCount(0);

		await page.getByRole('button', { name: 'Active', exact: true }).click();
		await expect(page.locator('tbody tr', { hasText: activeKeyName })).toBeVisible();
		await expect(page.locator('tbody tr', { hasText: revokeKeyName })).toHaveCount(0);

		// Search filter
		await page.getByRole('button', { name: 'All Status', exact: true }).click();
		await page.getByPlaceholder('Search keys...').fill(activeKeyName);
		await expect(page.locator('tbody tr', { hasText: activeKeyName })).toBeVisible();
		await expect(page.locator('tbody tr', { hasText: revokeKeyName })).toHaveCount(0);

		// Reload and verify revoked state persisted in backend
		await page.reload();
		const persistedRevokedRow = page.locator('tbody tr', { hasText: revokeKeyName });
		await expect(persistedRevokedRow).toBeVisible();
		await expect(persistedRevokedRow.getByText('Revoked', { exact: true })).toBeVisible();
	});
});
