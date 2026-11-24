import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
	ADMIN_EMAIL,
	ADMIN_PASSWORD,
	ADMIN_TOKEN,
	JWT_SECRET,
	RUNTIME_FILE,
	SEED_TEAM_NAME,
} from './constants';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const dashboardDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(dashboardDir, '../..');

export interface E2ERuntime {
	baseUrl: string;
	serverPid?: number;
	pgContainerId?: string;
	redisContainerId?: string;
}

function runSync(cmd: string, args: string[], cwd?: string): string {
	return execFileSync(cmd, args, {
		cwd,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
	}).trim();
}

function runInherited(cmd: string, args: string[], cwd: string): void {
	execFileSync(cmd, args, {
		cwd,
		stdio: 'inherit',
	});
}

async function waitFor(
	label: string,
	check: () => Promise<boolean> | boolean,
	timeoutMs: number,
	intervalMs = 500
): Promise<void> {
	const deadline = Date.now() + timeoutMs;
	let lastError: unknown;
	while (Date.now() < deadline) {
		try {
			if (await check()) return;
		} catch (err) {
			lastError = err;
		}
		await new Promise((r) => setTimeout(r, intervalMs));
	}
	throw new Error(
		`Timed out waiting for ${label} after ${timeoutMs}ms${lastError ? `: ${String(lastError)}` : ''}`
	);
}

function getMappedPort(containerId: string, containerPort: string): string {
	const out = runSync('docker', ['port', containerId, containerPort]);
	const firstLine = out.split('\n')[0]?.trim() || '';
	const port = firstLine.split(':').pop();
	if (!port) {
		throw new Error(`Could not determine mapped host port for ${containerId}:${containerPort} (got "${out}")`);
	}
	return port;
}

function stopContainer(id?: string): void {
	if (!id) return;
	try {
		execFileSync('docker', ['rm', '-f', id], { stdio: 'ignore' });
	} catch {
		// Best-effort cleanup
	}
}

export default async function globalSetup(): Promise<void> {
	const e2ePort = process.env.E2E_PORT || '3100';
	const baseUrl = process.env.E2E_BASE_URL || `http://127.0.0.1:${e2ePort}`;

	if (process.env.E2E_BASE_URL) {
		console.log(`[e2e] Using external server at ${baseUrl}`);
		await waitFor(
			`external server ${baseUrl}/healthz`,
			async () => {
				const res = await fetch(`${baseUrl}/healthz`);
				return res.ok;
			},
			30_000
		);
		return;
	}

	const runId = `hi-e2e-${Date.now()}-${Math.floor(Math.random() * 10000)}`;
	let pgContainerId: string | undefined;
	let redisContainerId: string | undefined;
	let serverProc: ChildProcess | undefined;

	try {
		console.log('[e2e] Starting ephemeral Postgres and Redis containers...');
		pgContainerId = runSync('docker', [
			'run',
			'-d',
			'--rm',
			'--name',
			`${runId}-pg`,
			'-e',
			'POSTGRES_USER=postgres',
			'-e',
			'POSTGRES_PASSWORD=postgres',
			'-e',
			'POSTGRES_DB=hyperinfer',
			'-p',
			'127.0.0.1::5432',
			'postgres:16-alpine',
		]);

		redisContainerId = runSync('docker', [
			'run',
			'-d',
			'--rm',
			'--name',
			`${runId}-redis`,
			'-p',
			'127.0.0.1::6379',
			'redis:7-alpine',
		]);

		const pgPort = getMappedPort(pgContainerId, '5432/tcp');
		const redisPort = getMappedPort(redisContainerId, '6379/tcp');

		await waitFor(
			'PostgreSQL readiness',
			() => {
				execFileSync(
					'docker',
					[
						'exec',
						pgContainerId!,
						'psql',
						'-h',
						'127.0.0.1',
						'-U',
						'postgres',
						'-d',
						'hyperinfer',
						'-c',
						'SELECT 1',
					],
					{ stdio: 'ignore' }
				);
				return true;
			},
			30_000
		);

		await waitFor(
			'Redis readiness',
			() => {
				const pong = runSync('docker', ['exec', redisContainerId!, 'redis-cli', 'ping']);
				return pong === 'PONG';
			},
			15_000
		);

		if (process.env.E2E_SKIP_BUILD !== '1') {
			console.log('[e2e] Building dashboard static bundle...');
			runInherited('bun', ['run', 'build'], dashboardDir);

			console.log('[e2e] Building hyperinfer-server with embedded-frontend...');
			runInherited(
				'cargo',
				['build', '--bin', 'hyperinfer-server', '--features', 'embedded-frontend'],
				repoRoot
			);
		}

		const serverBin =
			process.env.E2E_SERVER_BIN || path.join(repoRoot, 'target', 'debug', 'hyperinfer-server');
		if (!fs.existsSync(serverBin)) {
			throw new Error(`hyperinfer-server binary not found at ${serverBin}`);
		}

		const databaseUrl = `postgres://postgres:postgres@127.0.0.1:${pgPort}/hyperinfer`;
		const redisUrl = `redis://127.0.0.1:${redisPort}`;

		console.log(`[e2e] Starting hyperinfer-server on ${baseUrl}...`);
		const logPath = path.join(dashboardDir, 'e2e', '.e2e-server.log');
		const logStream = fs.openSync(logPath, 'w');

		serverProc = spawn(serverBin, [], {
			cwd: repoRoot,
			detached: false,
			stdio: ['ignore', logStream, logStream],
			env: {
				...process.env,
				BIND_ADDR: `127.0.0.1:${e2ePort}`,
				DATABASE_URL: databaseUrl,
				REDIS_URL: redisUrl,
				ADMIN_TOKEN,
				MCP_JWT_SECRET: JWT_SECRET,
				INITIAL_ADMIN_EMAIL: ADMIN_EMAIL,
				INITIAL_ADMIN_PASSWORD: ADMIN_PASSWORD,
				SEED_TEAM_NAME,
				AUTH_COOKIE_SECURE: 'false',
				ALLOWED_ORIGINS: `${baseUrl},http://localhost:${e2ePort}`,
				RUST_LOG: process.env.E2E_RUST_LOG || 'info',
			},
		});

		let fatalError: Error | null = null;
		await waitFor(
			`hyperinfer-server healthz at ${baseUrl}/healthz`,
			async () => {
				if (serverProc && serverProc.exitCode !== null) {
					const logs = fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf8') : '';
					fatalError = new Error(
						`hyperinfer-server exited prematurely (code ${serverProc.exitCode}):\n${logs}`
					);
					return true;
				}
				const res = await fetch(`${baseUrl}/healthz`);
				return res.ok;
			},
			60_000
		);
		if (fatalError) {
			throw fatalError;
		}

		const runtime: E2ERuntime = {
			baseUrl,
			serverPid: serverProc.pid,
			pgContainerId,
			redisContainerId,
		};
		fs.writeFileSync(RUNTIME_FILE, JSON.stringify(runtime, null, 2));
		console.log(`[e2e] Stack ready at ${baseUrl} (pid=${serverProc.pid})`);
	} catch (err) {
		if (serverProc?.pid) {
			try {
				process.kill(serverProc.pid, 'SIGTERM');
			} catch {
				// ignore
			}
		}
		stopContainer(pgContainerId);
		stopContainer(redisContainerId);
		throw err;
	}
}
