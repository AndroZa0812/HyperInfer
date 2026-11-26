import { test, expect } from './fixtures';

test.describe('Agent Sessions & Trajectory Observability (HYP-76)', () => {
	test('renders fleet sessions with attribution, cost/cache savings, MCP tool calls, and redacted trajectory inspection', async ({
		authedPage: page,
	}) => {
		// Seed a HyperCode agent session containing sensitive strings to verify server-side redaction
		const seedRes = await page.request.post('/v1/agent/sessions', {
			data: {
				session_key: 'hc-e2e-session-01',
				model: 'gpt-4o',
				status: 'completed',
				title: 'Investigate auth for bob@example.com using sk-proj-supersecretkey12345',
				input_tokens: 4000,
				cached_input_tokens: 2000,
				output_tokens: 1000,
				reasoning_tokens: 200,
				cache_hits: 3,
				total_latency_ms: 1820,
				mcp_tool_calls: [
					{
						server: 'github',
						tool: 'search_code',
						count: 4,
						avg_latency_ms: 95,
						errors: 0,
					},
				],
				trajectory: [
					{
						step: 1,
						timestamp: '2026-11-26T12:00:00Z',
						kind: 'prompt',
						role: 'user',
						content: 'Debug bob@example.com with sk-proj-supersecretkey12345',
						cache_hit: false,
					},
					{
						step: 2,
						timestamp: '2026-11-26T12:00:01Z',
						kind: 'mcp_tool_call',
						role: 'assistant',
						content: 'Invoking github.search_code for auth middleware',
						tool_name: 'search_code',
						mcp_server: 'github',
						tokens: 320,
						latency_ms: 95,
						cache_hit: true,
					},
				],
			},
		});
		expect(seedRes.status()).toBe(201);

		await page.goto('/dashboard/sessions');
		await expect(page.getByRole('heading', { name: 'Agent Sessions' })).toBeVisible();
		await expect(page.getByText('hc-e2e-session-01')).toBeVisible();
		await expect(page.getByText('gpt-4o')).toBeVisible();
		await expect(page.getByText('1,820 ms')).toBeVisible();

		// Raw secrets must never appear anywhere in the table
		await expect(page.getByText('bob@example.com')).toHaveCount(0);
		await expect(page.getByText('sk-proj-supersecretkey12345')).toHaveCount(0);
		await expect(page.getByText(/\[REDACTED_EMAIL\]/)).toBeVisible();

		// Open read-only Trajectory Inspector modal
		await page.getByRole('button', { name: 'Inspect' }).first().click();
		await expect(
			page.getByText(/Read-only redacted trajectory view/)
		).toBeVisible();
		await expect(
			page.getByText('github.search_code', { exact: true })
		).toBeVisible();
		await expect(
			page.getByText('Debug [REDACTED_EMAIL] with [REDACTED_API_KEY]')
		).toBeVisible();
		await expect(page.getByText('sk-proj-supersecretkey12345')).toHaveCount(0);
	});
});
