import { existsSync, writeFileSync } from 'node:fs';

const [name] = process.argv.slice(2);
if (process.argv.length !== 3 || !/^[a-z][a-z0-9_]*$/.test(name ?? '')) {
  throw new Error('Usage: npm run db:migration:add -- lower_snake_case_name');
}
const version = new Date().toISOString().replace(/[-:T]/g, '').slice(0, 14);
for (const direction of ['up', 'down']) {
  const path = `migrations/${version}_${name}.${direction}.sql`;
  if (existsSync(path)) throw new Error('Migration already exists');
  writeFileSync(path, `-- ${direction}: ${name}\n`, { flag: 'wx' });
  console.log(path);
}
