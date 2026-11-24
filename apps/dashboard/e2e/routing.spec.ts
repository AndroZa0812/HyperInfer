import { test, expect, uniqueName } from './fixtures';

test.describe('Routing Deployments', () => {
	test('creates, edits, filters, and deletes a deployment', async ({ authedPage: page }) => {
		const depName = uniqueName('dep-openai');
		const updatedName = `${depName}-updated`;

		await page.goto('/dashboard/routing');
		await expect(page.getByRole('heading', { name: 'Routing Deployments' })).toBeVisible();

		// Open Create Deployment modal
		await page.getByRole('button', { name: 'Add Deployment' }).click();
		const modal = page.getByRole('dialog');
		await expect(modal.getByRole('heading', { name: 'Create Deployment' })).toBeVisible();

		await modal.getByLabel('Name').fill(depName);
		await modal.getByLabel('Provider').selectOption('openai');
		await modal.getByLabel('Model').fill('gpt-4');
		await modal.getByLabel('Base URL').fill('https://api.openai.com/v1');
		await modal.getByLabel('API Key Ref').fill('env:OPENAI_API_KEY');
		await modal.getByLabel('Weight').fill('5');
		await modal.getByLabel('Priority').fill('1');

		await modal.getByRole('button', { name: 'Create', exact: true }).click();
		await expect(modal).toBeHidden();

		const row = page.locator('tbody tr', { hasText: depName });
		await expect(row).toBeVisible();
		await expect(row).toContainText('openai');
		await expect(row).toContainText('gpt-4');
		await expect(row).toContainText('5');
		await expect(row).toContainText('Active');

		// Reload and confirm persistence
		await page.reload();
		const persistedRow = page.locator('tbody tr', { hasText: depName });
		await expect(persistedRow).toBeVisible();

		// Edit deployment
		await persistedRow.getByRole('button', { name: 'Edit' }).click();
		await expect(modal.getByRole('heading', { name: 'Edit Deployment' })).toBeVisible();
		await modal.getByLabel('Name').fill(updatedName);
		await modal.getByLabel('Weight').fill('10');
		await modal.getByRole('button', { name: 'Update', exact: true }).click();
		await expect(modal).toBeHidden();

		const updatedRow = page.locator('tbody tr', { hasText: updatedName });
		await expect(updatedRow).toBeVisible();
		await expect(updatedRow).toContainText('10');

		// Filter by a model with no deployments
		const filterInput = page.getByPlaceholder('Filter by model...');
		await filterInput.fill('nonexistent-model-xyz');
		await filterInput.press('Enter');
		await expect(page.getByText('No deployments found for model "nonexistent-model-xyz"')).toBeVisible();

		// Filter back to gpt-4 and delete via styled confirmation modal
		await filterInput.fill('gpt-4');
		await filterInput.press('Enter');
		await expect(updatedRow).toBeVisible();

		await updatedRow.getByRole('button', { name: 'Delete' }).click();
		const confirmModal = page.getByRole('dialog');
		await expect(confirmModal.getByRole('heading', { name: 'Delete Deployment' })).toBeVisible();
		await confirmModal.getByRole('button', { name: 'Delete', exact: true }).click();
		await expect(confirmModal).toBeHidden();
		await expect(page.locator('tbody tr', { hasText: updatedName })).toHaveCount(0);
	});
});
