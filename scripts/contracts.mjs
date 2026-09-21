import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const check = process.argv.includes('--check');
function run(command, args) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
  return result.stdout;
}

function saveOrCheck(path, content) {
  if (check) {
    if (readFileSync(path, 'utf8').replaceAll('\r\n', '\n') !== content.replaceAll('\r\n', '\n')) {
      throw new Error(`Contract drift: ${path}. Run npm run contracts:generate.`);
    }
  } else {
    writeFileSync(path, content);
  }
}

const json = run('cargo', ['run', '--locked', '--quiet', '--bin', 'export-openapi']);
saveOrCheck('packages/contracts/openapi.json', json);
const { default: openapiTS, astToString } = await import('openapi-typescript');
const ast = await openapiTS(JSON.parse(json));
saveOrCheck(
  fileURLToPath(new URL('../packages/contracts/src/schema.d.ts', import.meta.url)),
  astToString(ast),
);
console.log(check ? 'OpenAPI and TypeScript contracts match the backend.' : 'Contracts generated.');
