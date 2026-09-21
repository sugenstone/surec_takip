import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';

// Every run owns a fresh project/volume and deletes only those resources in finally.
const project = `surec-smoke-${randomBytes(4).toString('hex')}`;
const env = { ...process.env, POSTGRES_PORT: '25432', API_PORT: '28080', WEB_PORT: '23000' };
async function compose(...args) {
  await new Promise((resolve, reject) => {
    const child = spawn('docker', ['compose', '-p', project, ...args], { stdio: 'inherit', env });
    child.on('error', reject);
    child.on('exit', (code) =>
      code === 0 ? resolve() : reject(new Error(`Compose ${args[0]} failed: ${code}`)),
    );
  });
}

async function probe(path, status, base = 'http://127.0.0.1:28080') {
  const response = await fetch(`${base}${path}`);
  assert.equal(response.status, status, path);
  return response;
}

try {
  await compose('up', '--build', '--detach', '--wait', '--wait-timeout', '180');
  await probe('/api/v1/health', 200);
  await probe('/api/v1/ready', 200);
  await probe('/', 200, 'http://127.0.0.1:23000');
  await compose('run', '--rm', 'migrate', '/app/migrate', 'verify');
  await compose('stop', 'db');
  await probe('/api/v1/health', 200);
  const unavailable = await probe('/api/v1/ready', 503);
  assert.equal((await unavailable.json()).error.code, 'SERVICE_NOT_READY');
  await compose('start', 'db');
  let recovered = false;
  for (let attempt = 0; attempt < 30; attempt++) {
    if ((await fetch('http://127.0.0.1:28080/api/v1/ready')).status === 200) {
      recovered = true;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  assert.ok(recovered, 'Readiness must recover after PostgreSQL restarts');
  console.log('Clean-volume Docker startup, migration, HTTP and recovery checks passed.');
} finally {
  await compose('down', '--volumes', '--remove-orphans');
}
