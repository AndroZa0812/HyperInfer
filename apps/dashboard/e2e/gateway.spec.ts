import { test, expect, uniqueName } from './fixtures';

test.describe('Gateway Views (MCP, Playground, Runtime Config)', () => {
	test('manages MCP upstream servers and per-team tool policies (HYP-70)', async ({
		authedPage: page,
	}) => {
		const upstreamName = uniqueName('mcp_srv').toLowerCase().replace(/[^a-z0-9_-]/g, '_');

		await page.goto('/dashboard/mcp');
		await expect(page.getByRole('heading', { name: 'MCP Gateway' })).toBeVisible();

		// Register upstream server with a secret Authorization header
		await page.getByRole('button', { name: /Add Upstream Server/ }).click();
		const dialog = page.getByRole('dialog');
		await expect(dialog.getByRole('heading', { name: 'Add Upstream MCP Server' })).toBeVisible();
		await dialog.getByLabel('Server Name').fill(upstreamName);
		await dialog.getByLabel('Endpoint URL').fill('https://mcp.example.invalid/mcp');
		await dialog.getByLabel('Authorization Header (optional)').fill('Bearer super-secret-e2e-token');
		await dialog.getByRole('button', { name: 'Add Server' }).click();
		await expect(dialog).toBeHidden();

		const row = page.locator('tbody tr', { hasText: upstreamName });
		await expect(row).toBeVisible();
		await expect(row).toContainText('Authorization');
		// Secret header value must never be rendered in the UI
		await expect(page.locator('body')).not.toContainText('super-secret-e2e-token');

		// Configure per-team regex allow/deny policy + Catalog Mode
		await page.getByLabel('Allow Patterns (one regex per line)').fill('^github__.*');
		await page.getByLabel('Deny Patterns (one regex per line)').fill('github__delete_.*');
		await page.getByLabel(/Catalog \/ Code Mode/).check();
		await page.getByRole('button', { name: 'Save Policy' }).click();
		await expect(page.getByText('Policy saved')).toBeVisible();

		// Reload and confirm upstream & policy persisted
		await page.reload();
		await expect(page.locator('tbody tr', { hasText: upstreamName })).toBeVisible();
		await expect(page.getByLabel('Allow Patterns (one regex per line)')).toHaveValue('^github__.*');
		await expect(page.getByLabel('Deny Patterns (one regex per line)')).toHaveValue('github__delete_.*');
		await expect(page.getByLabel(/Catalog \/ Code Mode/)).toBeChecked();

		// Remove upstream server via confirmation modal
		await page.locator('tbody tr', { hasText: upstreamName }).getByRole('button', { name: 'Remove' }).click();
		const confirmDialog = page.getByRole('dialog');
		await expect(confirmDialog.getByRole('heading', { name: 'Remove Upstream Server' })).toBeVisible();
		await confirmDialog.getByRole('button', { name: 'Remove', exact: true }).click();
		await expect(confirmDialog).toBeHidden();
		await expect(page.locator('tbody tr', { hasText: upstreamName })).toHaveCount(0);
	});

	test('switches Multi-SDK drop-in snippets in Playground (HYP-72)', async ({
		authedPage: page,
	}) => {
		await page.goto('/dashboard/playground');
		await expect(page.getByRole('heading', { name: 'Playground' })).toBeVisible();

		// OpenAI SDK default snippet
		await expect(page.locator('pre')).toContainText('from openai import OpenAI');
		await expect(page.locator('pre')).toContainText('/v1');

		// Switch to Anthropic / Claude Code
		await page.getByRole('button', { name: 'Anthropic / Claude Code' }).click();
		await expect(page.locator('pre')).toContainText('ANTHROPIC_BASE_URL');

		// Switch to Gemini SDK
		await page.getByRole('button', { name: 'Gemini SDK' }).click();
		await expect(page.locator('pre')).toContainText('/v1beta/models/');

		// Switch to Responses API
		await page.getByRole('button', { name: 'Responses API' }).click();
		await expect(page.locator('pre')).toContainText('/v1/responses');

		// Validation when sending without a Virtual API Key
		await page.getByLabel('User Message').fill('Hello');
		await page.getByRole('button', { name: /Send Request/ }).click();
		await expect(page.getByText(/Enter a Virtual API Key/)).toBeVisible();
	});

	test('exposes server runtime config and onboarding card on Overview (HYP-74)', async ({
		authedPage: page,
	}) => {
		const res = await page.request.get('/v1/auth/config');
		expect(res.ok()).toBeTruthy();
		const cfg = await res.json();
		expect(cfg.mode).toBe('standard');
		expect(cfg.endpoints.anthropic_messages).toBe('/v1/messages');
		expect(cfg.endpoints.mcp_http).toBe('/mcp');

		await page.goto('/dashboard');
		await expect(page.getByText('Gateway Runtime')).toBeVisible();
		await expect(page.getByText('Standard Mode')).toBeVisible();

		// Dynamic model catalog endpoints (HYP-77)
		const modelsRes = await page.request.get('/v1/models');
		expect(modelsRes.ok()).toBeTruthy();
		const modelsJson = await modelsRes.json();
		expect(modelsJson.object).toBe('list');

		const catalogRes = await page.request.get('/v1/admin/catalog');
		expect(catalogRes.ok()).toBeTruthy();
		const catalogJson = await catalogRes.json();
		expect(Array.isArray(catalogJson.models)).toBeTruthy();

		await page.goto('/dashboard/routing');
		await expect(page.getByRole('button', { name: 'Sync Catalog' })).toBeVisible();
	});
});
