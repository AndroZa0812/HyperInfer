import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { RUNTIME_FILE } from './constants';
import type { E2ERuntime } from './global-setup';

function stopContainer(id?: string): void {
	if (!id) return;
	try {
		execFileSync('docker', ['rm', '-f', id], { stdio: 'ignore' });
	} catch {
		// Best-effort cleanup
	}
}

export default async function globalTeardown(): Promise<void> {
	if (!fs.existsSync(RUNTIME_FILE)) {
		return;
	}

	let runtime: E2ERuntime;
	try {
		runtime = JSON.parse(fs.readFileSync(RUNTIME_FILE, 'utf8')) as E2ERuntime;
	} finally {
		fs.rmSync(RUNTIME_FILE, { force: true });
	}

	if (runtime.serverPid) {
		try {
			process.kill(runtime.serverPid, 'SIGTERM');
		} catch {
			// Process may already have exited
		}
	}

	stopContainer(runtime.pgContainerId);
	stopContainer(runtime.redisContainerId);
	console.log('[e2e] Teardown complete');
}
