import { existsSync, writeFileSync } from 'node:fs';
import { randomBytes } from 'node:crypto';

if (existsSync('.env')) {
  console.log('.env already exists; no values changed.');
} else {
  const password = randomBytes(24).toString('hex');
  writeFileSync(
    '.env',
    [
      'POSTGRES_USER=platform',
      `POSTGRES_PASSWORD=${password}`,
      'POSTGRES_DB=platform_dev',
      'POSTGRES_PORT=15432',
      `DATABASE_URL=postgres://platform:${password}@127.0.0.1:15432/platform_dev`,
      `TEST_DATABASE_URL=postgres://platform:${password}@127.0.0.1:15432/postgres`,
      'DATABASE_MAX_CONNECTIONS=10',
      'SERVER_BIND=127.0.0.1:8080',
      'RUST_LOG=platform_server=info',
      'API_PORT=8080',
      'WEB_PORT=3000',
      // Local development runs over plain HTTP; production sets true.
      'SESSION_COOKIE_SECURE=false',
      '',
    ].join('\n'),
    { flag: 'wx', mode: 0o600 },
  );
  console.log('Created local .env with a random database password.');
}
