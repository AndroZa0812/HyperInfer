import { test, expect } from '@playwright/test';
import { spawn, execSync, type ChildProcess } from 'node:child_process';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const WEB_PORT = 3219;
const MOCK_LLM_PORT = 3229;
const AUTH_TOKEN = 'e2e-hypercode-web-secret-token';

async function waitForHealth(url: string, timeoutMs = 30000) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		try {
			const res = await fetch(url);
			if (res.ok) return;
		} catch {
			// retry
		}
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error(`Timed out waiting for ${url}`);
}

test.describe('HyperCode Web UI (HYP-75)', () => {
	let mockLlm: http.Server;
	let daemon: ChildProcess;
	let tmpDir: string;

	test.beforeAll(async () => {
		test.setTimeout(120_000);
		// Create temporary git workspace for live diff verification
		tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'hypercode-web-e2e-'));
		fs.writeFileSync(path.join(tmpDir, 'notes.txt'), 'initial line\n');
		execSync('git init && git add notes.txt && git -c user.name=E2E -c user.email=e2e@test commit -m init', {
			cwd: tmpDir,
			stdio: 'ignore'
		});

		// Deterministic mock OpenAI-compatible LLM server (supports both streaming SSE and JSON)
		let callCount = 0;
		mockLlm = http.createServer((req, res) => {
			let body = '';
			req.on('data', (chunk) => (body += chunk));
			req.on('end', () => {
				callCount += 1;
				let isStream = false;
				try {
					isStream = Boolean(JSON.parse(body).stream);
				} catch {
					// ignore
				}

				if (isStream) {
					res.writeHead(200, {
						'Content-Type': 'text/event-stream',
						'Cache-Control': 'no-cache',
						Connection: 'keep-alive'
					});
					if (callCount === 1) {
						const args = JSON.stringify({
							path: 'notes.txt',
							content: 'initial line\nadded by hypercode web\n'
						});
						res.write(
							`data: ${JSON.stringify({
								id: 'chatcmpl-1',
								object: 'chat.completion.chunk',
								model: 'mock-coder',
								choices: [
									{
										index: 0,
										delta: { role: 'assistant', content: 'Updating notes.txt now.' },
										finish_reason: null
									}
								]
							})}\n\n`
						);
						res.write(
							`data: ${JSON.stringify({
								id: 'chatcmpl-1',
								object: 'chat.completion.chunk',
								model: 'mock-coder',
								choices: [
									{
										index: 0,
										delta: {
											tool_calls: [
												{
													index: 0,
													id: 'call_write_1',
													type: 'function',
													function: { name: 'write_file', arguments: args }
												}
											]
										},
										finish_reason: 'tool_calls'
									}
								]
							})}\n\n`
						);
						res.end('data: [DONE]\n\n');
					} else {
						res.write(
							`data: ${JSON.stringify({
								id: 'chatcmpl-2',
								object: 'chat.completion.chunk',
								model: 'mock-coder',
								choices: [
									{
										index: 0,
										delta: { role: 'assistant', content: 'Done! I updated notes.txt for you.' },
										finish_reason: 'stop'
									}
								]
							})}\n\n`
						);
						res.end('data: [DONE]\n\n');
					}
					return;
				}

				res.setHeader('Content-Type', 'application/json');
				if (callCount === 1) {
					res.end(
						JSON.stringify({
							id: 'chatcmpl-1',
							object: 'chat.completion',
							model: 'mock-coder',
							choices: [
								{
									index: 0,
									message: {
										role: 'assistant',
										content: 'Updating notes.txt now.',
										tool_calls: [
											{
												id: 'call_write_1',
												type: 'function',
												function: {
													name: 'write_file',
													arguments: JSON.stringify({
														path: 'notes.txt',
														content: 'initial line\nadded by hypercode web\n'
													})
												}
											}
										]
									},
									finish_reason: 'tool_calls'
								}
							],
							usage: { prompt_tokens: 50, completion_tokens: 20, total_tokens: 70 }
						})
					);
				} else {
					res.end(
						JSON.stringify({
							id: 'chatcmpl-2',
							object: 'chat.completion',
							model: 'mock-coder',
							choices: [
								{
									index: 0,
									message: {
										role: 'assistant',
										content: 'Done! I updated notes.txt for you.'
									},
									finish_reason: 'stop'
								}
							],
							usage: { prompt_tokens: 80, completion_tokens: 15, total_tokens: 95 }
						})
					);
				}
			});
		});
		await new Promise<void>((resolve) => mockLlm.listen(MOCK_LLM_PORT, '127.0.0.1', resolve));

		// Build hypercode with embedded-web feature and start localhost daemon
		const repoRoot = path.resolve(import.meta.dirname, '../../..');
		execSync('cargo build -p hypercode --features embedded-web', {
			cwd: repoRoot,
			stdio: 'inherit'
		});

		const binPath = path.join(repoRoot, 'target/debug/hypercode');
		daemon = spawn(
			binPath,
			[
				'web',
				'--port',
				String(WEB_PORT),
				'--token',
				AUTH_TOKEN,
				'--no-open',
				'--workspace',
				tmpDir,
				'--model',
				'mock-coder',
				'--local-url',
				`http://127.0.0.1:${MOCK_LLM_PORT}/v1`,
				'--approval',
				'mutations'
			],
			{
				cwd: tmpDir,
				stdio: 'pipe'
			}
		);
		daemon.stderr?.on('data', (d) => process.stderr.write(d));

		await waitForHealth(`http://127.0.0.1:${WEB_PORT}/api/health`);
	});

	test.afterAll(async () => {
		daemon?.kill('SIGTERM');
		await new Promise<void>((resolve) => mockLlm?.close(() => resolve()));
		if (tmpDir) {
			fs.rmSync(tmpDir, { recursive: true, force: true });
		}
	});

	test('enforces auth token, streams agent turn, prompts for tool approval, and updates live file diff & command output', async ({
		page
	}) => {
		// Unauthenticated API calls are rejected
		const unauth = await page.request.get('/api/info');
		expect(unauth.status()).toBe(401);

		// Open UI with tokenized URL -> auto-authenticates and strips token from URL
		await page.goto(`/?token=${AUTH_TOKEN}`);
		await expect(page.getByText('Interactive Agent')).toBeVisible();
		expect(page.url()).not.toContain(AUTH_TOKEN);

		// Send a prompt that triggers a write_file tool call requiring approval
		await page.getByPlaceholder('Describe a task for HyperCode...').fill('Update notes.txt');
		await page.getByRole('button', { name: 'Send' }).click();

		// Approval gate card appears
		await expect(page.getByText('Approval Required')).toBeVisible();
		await expect(page.getByText('write_file', { exact: true }).first()).toBeVisible();

		// Approve tool execution
		await page.getByRole('button', { name: 'Approve' }).click();

		// Assistant completes the turn
		await expect(page.getByText('Done! I updated notes.txt for you.')).toBeVisible();

		// Live Diffs tab reflects the file change automatically
		await expect(page.getByText('notes.txt').first()).toBeVisible();
		await expect(page.getByText('+added by hypercode web')).toBeVisible();

		// Command Output tab shows the write_file execution
		await page.getByRole('button', { name: /Output \(/ }).click();
		await expect(page.getByText('write_file').first()).toBeVisible();

		// Subagents tab renders cleanly
		await page.getByRole('button', { name: /Subagents \(/ }).click();
		await expect(page.getByText('No subagents spawned in this session.')).toBeVisible();
	});
});
