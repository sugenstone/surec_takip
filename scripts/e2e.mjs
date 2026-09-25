import { randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
// Direct node invocation avoids npx/.cmd shim differences across platforms.
const playwrightCli = join(repoRoot, 'node_modules', 'playwright', 'cli.js');

// Full-stack browser test runner: every run owns an isolated Compose project
// (PostgreSQL + migrate + server on dedicated ports), seeds one user via
// user-admin, then runs Playwright against the Vite dev server whose /api
// proxy and SSR origin point at this stack. Teardown removes only its own
// project/volume in finally.
const project = `surec-e2e-${randomBytes(4).toString('hex')}`;
const API_PORT = '28081';
const env = {
  ...process.env,
  E2E_COMPOSE_PROJECT: project,
  POSTGRES_PORT: '25433',
  API_PORT,
  // The Vite /api proxy (browser) and SvelteKit SSR both target this stack.
  API_PROXY_TARGET: `http://127.0.0.1:${API_PORT}`,
  API_ORIGIN: `http://127.0.0.1:${API_PORT}`,
};

async function compose(...args) {
  await new Promise((resolve, reject) => {
    const child = spawn('docker', ['compose', '-p', project, ...args], {
      stdio: 'inherit',
      env,
    });
    child.on('error', reject);
    child.on('exit', (code) =>
      code === 0 ? resolve() : reject(new Error(`Compose ${args[0]} failed: ${code}`)),
    );
  });
}

async function playwright() {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [playwrightCli, 'test', ...process.argv.slice(2)], {
      stdio: 'inherit',
      env,
      cwd: repoRoot,
    });
    // Resolve with the code; process.exit here would skip the finally teardown.
    child.on('error', () => resolve(1));
    child.on('exit', (code) => resolve(code ?? 1));
  });
}

try {
  await compose('up', '--build', '--detach', '--wait', '--wait-timeout', '180', 'db', 'server');
  // Distinct users keep spec fixtures isolated from each other: auth/shell
  // (e2e), projects owner (e2e2), invited permission-less member (e2e3) and
  // sections owner (e2e4), work items owner (e2e5), UX fixtures (e2e6),
  // processes owner (e2e7), executions owner (e2e8) and progress (e2e9).
  for (const [email, displayName] of [
    ['e2e@example.test', 'E2e User'],
    ['e2e2@example.test', 'E2e User Two'],
    ['e2e3@example.test', 'E2e User Three'],
    ['e2e4@example.test', 'E2e User Four'],
    ['e2e5@example.test', 'E2e User Five'],
    ['e2e6@example.test', 'E2e UX Review'],
    ['e2e7@example.test', 'E2e Processes'],
    ['e2e8@example.test', 'E2e Executions'],
    ['e2e9@example.test', 'E2e Progress'],
  ]) {
    await compose(
      'run',
      '--rm',
      '-e',
      'USER_PASSWORD=e2e-password-1',
      'server',
      '/app/user-admin',
      'create',
      email,
      displayName,
    );
  }
  process.exitCode = await playwright();
} finally {
  await compose('down', '--volumes', '--remove-orphans');
}
