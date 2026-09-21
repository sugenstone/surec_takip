import { spawn } from 'node:child_process';

const args = process.argv.slice(2);
if (args[0] === '--test-db') {
  args.shift();
  if (!process.env.TEST_DATABASE_URL) throw new Error('TEST_DATABASE_URL is required');
  process.env.DATABASE_URL = process.env.TEST_DATABASE_URL;
}
const child = spawn('cargo', args, { stdio: 'inherit', env: process.env });
child.on('error', () => {
  console.error('Cargo could not be started');
  process.exitCode = 1;
});
child.on('exit', (code) => {
  process.exitCode = code ?? 1;
});
