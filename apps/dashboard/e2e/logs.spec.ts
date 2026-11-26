import { test, expect } from './fixtures';

test.describe('Request Logs Explorer (HYP-71)', () => {
	test('renders /dashboard/logs, supports filters, live tail toggle, and paginated logs API', async ({
		authedPage: page,
	}) => {
		await page.goto('/dashboard/logs');
		await expect(page.getByRole('heading', { name: 'Request Logs' })).toBeVisible();
		await expect(page.getByText('Matching Requests')).toBeVisible();
		await expect(page.getByText('No request logs match the current filters.')).toBeVisible();

		// Exercise filter controls
		await page.getByLabel('Model').fill('claude');
		await page.getByLabel('Status').selectOption('200');
		await page.getByLabel('Cache').selectOption('hit');
		await page.getByRole('button', { name: 'Apply Filters' }).click();
		await expect(page.getByText('No request logs match the current filters.')).toBeVisible();

		// Reset filters & toggle Live Tail
		await page.getByRole('button', { name: 'Reset' }).click();
		await expect(page.getByLabel('Model')).toHaveValue('');
		const liveTail = page.getByLabel(/Live Tail/);
		await liveTail.check();
		await expect(liveTail).toBeChecked();
		await liveTail.uncheck();

		// Verify paginated API endpoint directly
		const res = await page.request.get('/v1/usage/logs?limit=10&offset=0&cache_hit=false');
		expect(res.ok()).toBeTruthy();
		const body = await res.json();
		expect(Array.isArray(body.logs)).toBeTruthy();
		expect(typeof body.total).toBe('number');
		expect(body.limit).toBe(10);
		expect(body.offset).toBe(0);
	});
});
