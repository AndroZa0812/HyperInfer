import { test, expect, ADMIN_EMAIL, ADMIN_PASSWORD, loginViaUi } from './fixtures';

test.describe('Settings', () => {
	test('displays profile info and toggles theme in preferences', async ({ authedPage: page }) => {
		await page.goto('/dashboard/settings');
		await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();

		// Profile tab is active by default
		await expect(page.getByRole('heading', { name: 'Account Overview' })).toBeVisible();
		await expect(page.locator('input[disabled]').first()).toHaveValue(ADMIN_EMAIL);
		await expect(page.getByText('admin', { exact: true }).first()).toBeVisible();
		await expect(page.getByText('Keys Managed')).toHaveCount(0);

		// Preferences tab: switch theme to Dark and back to Light
		await page.getByRole('button', { name: 'Preferences', exact: true }).click();
		await expect(page.getByRole('heading', { name: 'Appearance' })).toBeVisible();

		await page.getByRole('button', { name: /Dark/ }).last().click();
		await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

		await page.reload();
		await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

		await page.getByRole('button', { name: 'Preferences', exact: true }).click();
		await page.getByRole('button', { name: /Light/ }).last().click();
		await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
	});

	test('validates and updates password in security tab', async ({ authedPage: page }) => {
		const newPassword = 'UpdatedPassword456!';

		await page.goto('/dashboard/settings');
		await page.getByRole('button', { name: 'Security', exact: true }).click();
		await expect(page.getByRole('heading', { name: 'Change Password' })).toBeVisible();

		const currentInput = page.locator('input[type="password"]').nth(0);
		const newInput = page.locator('input[type="password"]').nth(1);
		const confirmInput = page.locator('input[type="password"]').nth(2);
		const submitBtn = page.getByRole('button', { name: 'Update Password' });

		// Client-side validation: too short
		await currentInput.fill(ADMIN_PASSWORD);
		await newInput.fill('short');
		await confirmInput.fill('short');
		await submitBtn.click();
		await expect(page.getByText('New password must be at least 8 characters')).toBeVisible();

		// Client-side validation: mismatch
		await newInput.fill(newPassword);
		await confirmInput.fill('DifferentPass123!');
		await submitBtn.click();
		await expect(page.getByText('Passwords do not match')).toBeVisible();

		// Server-side validation: wrong current password
		await currentInput.fill('WrongCurrent123!');
		await newInput.fill(newPassword);
		await confirmInput.fill(newPassword);
		await submitBtn.click();
		await expect(page.getByText('Current password is incorrect')).toBeVisible();

		// Valid password change
		await currentInput.fill(ADMIN_PASSWORD);
		await newInput.fill(newPassword);
		await confirmInput.fill(newPassword);
		await submitBtn.click();
		await expect(page.getByText('Password updated successfully')).toBeVisible();

		try {
			// Logout and verify login with new password works
			await page.locator('aside').getByRole('button', { name: /Logout/ }).click();
			await expect(page).toHaveURL(/\/login$/);

			await loginViaUi(page, ADMIN_EMAIL, newPassword);
			await expect(page).toHaveURL(/\/dashboard$/);
		} finally {
			// Always restore original seeded password so subsequent runs remain clean
			await page.request.post('/v1/auth/change-password', {
				data: {
					current_password: newPassword,
					new_password: ADMIN_PASSWORD,
				},
			});
		}
	});

	test('configures PII redaction, runs live preview, and saves semantic cache settings (HYP-73)', async ({
		authedPage: page,
	}) => {
		await page.goto('/dashboard/settings');
		await page.getByRole('button', { name: 'Plugins & Cache', exact: true }).click();
		await expect(page.getByRole('heading', { name: 'PII & Secret Redaction' })).toBeVisible();

		// Add custom redaction pattern and save
		await page.getByLabel(/Custom Regex Rules/).fill('SECRET-[0-9]+ => [CUSTOM_SECRET]');
		await page.getByRole('button', { name: 'Save Redaction' }).click();
		await expect(page.getByText('Redaction settings saved')).toBeVisible();

		// Live redaction preview
		await page.getByPlaceholder(/Paste sample text/).fill('Email alice@example.com with SECRET-42');
		await page.getByRole('button', { name: 'Test Redaction' }).click();
		await expect(page.getByText('Email [REDACTED_EMAIL] with [CUSTOM_SECRET]')).toBeVisible();

		// Update semantic cache settings
		await page.getByLabel('Enable Semantic Cache').check();
		await page.getByLabel('Cache TTL (seconds)').fill('1800');
		await page.getByRole('button', { name: 'Save Cache Settings' }).click();
		await expect(page.getByText('Cache settings saved')).toBeVisible();

		// Reload and confirm persistence
		await page.reload();
		await page.getByRole('button', { name: 'Plugins & Cache', exact: true }).click();
		await expect(page.getByLabel('Enable Semantic Cache')).toBeChecked();
		await expect(page.getByLabel('Cache TTL (seconds)')).toHaveValue('1800');
	});
});
