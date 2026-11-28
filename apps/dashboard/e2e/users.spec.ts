import {
	test,
	expect,
	ADMIN_EMAIL,
	ADMIN_PASSWORD,
	loginViaUi,
	uniqueName,
} from './fixtures';

test.describe('Users & Member RBAC', () => {
	test('creates a member user, verifies member RBAC restrictions, updates role, and deletes user', async ({
		authedPage: page,
	}) => {
		const memberEmail = `${uniqueName('member').toLowerCase()}@hyperinfer.test`;
		const memberPassword = 'MemberPass123!';

		// 1. Admin creates a member user via /dashboard/users (HYP-55)
		await page.goto('/dashboard/users');
		await expect(page.getByRole('heading', { name: 'Users', exact: true })).toBeVisible();
		await expect(page.locator('tbody tr', { hasText: ADMIN_EMAIL })).toBeVisible();

		await page.getByRole('button', { name: /Create User/ }).click();
		const createDialog = page.getByRole('dialog');
		await expect(createDialog.getByRole('heading', { name: 'Create User' })).toBeVisible();
		await createDialog.getByLabel('Email Address').fill(memberEmail);
		await createDialog.getByLabel('Role').click();
		await page.getByRole('option', { name: 'Member' }).click();
		await createDialog.getByLabel('Initial Password').fill(memberPassword);
		await createDialog.getByRole('button', { name: 'Create User' }).click();
		await expect(createDialog).toBeHidden();

		const memberRow = page.locator('tbody tr', { hasText: memberEmail });
		await expect(memberRow).toBeVisible();
		await expect(memberRow).toContainText('member');

		// 2. Toggle role member -> admin -> member
		await memberRow.getByRole('button', { name: 'Make Admin' }).click();
		await expect(memberRow).toContainText('admin');
		await memberRow.getByRole('button', { name: 'Make Member' }).click();
		await expect(memberRow).toContainText('member');

		// 3. Log out and sign in as the member user to verify RBAC (HYP-60)
		await page.locator('aside').getByRole('button', { name: /Logout/ }).click();
		await expect(page).toHaveURL(/\/login$/);

		await loginViaUi(page, memberEmail, memberPassword);
		await expect(page).toHaveURL(/\/dashboard$/);
		await expect(page.getByRole('heading', { name: 'Overview' })).toBeVisible();

		// Admin-only nav links are hidden for member
		await expect(page.locator('aside nav').getByRole('link', { name: 'Overview' })).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Keys' })).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Settings' })).toBeVisible();
		await expect(page.locator('aside nav').getByRole('link', { name: 'Teams' })).toHaveCount(0);
		await expect(page.locator('aside nav').getByRole('link', { name: 'Users' })).toHaveCount(0);
		await expect(page.locator('aside nav').getByRole('link', { name: 'Routing' })).toHaveCount(0);

		// Member attempting admin-only API mutations gets 403 Forbidden
		const createTeamRes = await page.request.post('/v1/teams', {
			data: { name: uniqueName('ForbiddenTeam'), budget_cents: 1000 },
		});
		expect(createTeamRes.status()).toBe(403);

		const listUsersRes = await page.request.get('/v1/users');
		expect(listUsersRes.status()).toBe(403);

		// 4. Log back in as admin and delete the test user
		await page.locator('aside').getByRole('button', { name: /Logout/ }).click();
		await expect(page).toHaveURL(/\/login$/);

		await loginViaUi(page, ADMIN_EMAIL, ADMIN_PASSWORD);
		await page.goto('/dashboard/users');
		const targetRow = page.locator('tbody tr', { hasText: memberEmail });
		await expect(targetRow).toBeVisible();

		await targetRow.getByRole('button', { name: 'Delete' }).click();
		const deleteDialog = page.getByRole('dialog');
		await expect(deleteDialog.getByRole('heading', { name: 'Delete User' })).toBeVisible();
		await deleteDialog.getByRole('button', { name: 'Delete', exact: true }).click();
		await expect(deleteDialog).toBeHidden();
		await expect(page.locator('tbody tr', { hasText: memberEmail })).toHaveCount(0);
	});
});
