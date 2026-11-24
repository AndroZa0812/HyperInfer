import { test, expect, uniqueName } from './fixtures';

test.describe('Routing Deployments', () => {
	test('creates, edits, filters, and deletes a deployment', async ({ authedPage: page }) => {
		const depName = uniqueName('dep-openai');
		const updatedName = `${depName}-updated`;

		await page.goto('/dashboard/routing');
		await expect(page.getByRole('heading', { name: 'Routing Deployments' })).toBeVisible();

		// Open Create Deployment modal
		await page.getByRole('button', { name: 'Add Deployment' }).click();
		const modal = page.locator('.fixed.inset-0');
		await expect(modal.getByRole('heading', { name: 'Create Deployment' })).toBeVisible();

		await modal.locator('div:has(> label:text-is("Name")) input').fill(depName);
		await modal.locator('div:has(> label:text-is("Provider")) select').selectOption('openai');
		await modal.locator('div:has(> label:text-is("Model")) input').fill('gpt-4');
		await modal
			.locator('div:has(> label:text-is("Base URL")) input')
			.fill('https://api.openai.com/v1');
		await modal.locator('div:has(> label:text-is("API Key Ref")) input').fill('env:OPENAI_API_KEY');
		await modal.locator('div:has(> label:text-is("Weight")) input').fill('5');
		await modal.locator('div:has(> label:text-is("Priority")) input').fill('1');

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
		await modal.locator('div:has(> label:text-is("Name")) input').fill(updatedName);
		await modal.locator('div:has(> label:text-is("Weight")) input').fill('10');
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

		// Filter back to gpt-4 and delete
		await filterInput.fill('gpt-4');
		await filterInput.press('Enter');
		await expect(updatedRow).toBeVisible();

		page.once('dialog', (dialog) => dialog.accept());
		await updatedRow.getByRole('button', { name: 'Delete' }).click();
		await expect(page.locator('tbody tr', { hasText: updatedName })).toHaveCount(0);
	});
});
