// Installs both references' dependencies and builds their data indexes, which
// are generated, not committed. Uses `npm ci` (never `npm install`, which would
// rewrite the lockfile inside a submodule). Everything written is gitignored.
import { execSync } from 'node:child_process'
import path from 'node:path'

const ref = (name) => path.resolve(import.meta.dirname, '../../../ref', name, 'renderer')
const run = (cmd, cwd) => {
  console.log(`$ (${path.relative(process.cwd(), cwd) || '.'}) ${cmd}`)
  execSync(cmd, { cwd, stdio: 'inherit' })
}

for (const name of ['apt', 'ee2']) {
  run('npm ci --no-audit --no-fund', ref(name))
}
// PoE1 has no npm script; the module runs itself when executed directly.
run('node src/assets/make-index-files.mjs', ref('apt'))
run('npm run make-index-files', ref('ee2'))
