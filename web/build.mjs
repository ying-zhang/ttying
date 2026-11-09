import { build } from 'esbuild';
import { copyFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('.', import.meta.url));
await build({ absWorkingDir: root, entryPoints: ['src/main.js'], bundle: true, minify: true,
  outfile: '../crates/ttying-core/assets/app.js' });
await copyFile(new URL('src/index.html', import.meta.url), new URL('../crates/ttying-core/assets/index.html', import.meta.url));
