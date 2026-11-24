import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const ADMIN_EMAIL = process.env.E2E_ADMIN_EMAIL || 'admin@hyperinfer.test';
export const ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD || 'TestPassword123!';
export const ADMIN_TOKEN = process.env.E2E_ADMIN_TOKEN || 'e2e-admin-token-secret';
export const JWT_SECRET = process.env.E2E_JWT_SECRET || 'e2e-jwt-secret-key-min-32-bytes-long';
export const SEED_TEAM_NAME = process.env.E2E_SEED_TEAM_NAME || 'Default Team';
export const RUNTIME_FILE = path.join(__dirname, '.e2e-runtime.json');
