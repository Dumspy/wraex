// Builds a reference renderer into tools/golden/.screens-build/<game>/.
//
// The reference's own vite does the work, with cwd inside ref/ but the output
// outside it, so nothing is written into the submodule except the generated
// data indexes (`golden:prepare` builds those; PoE1's vite plugin rebuilds them
// on its own buildStart anyway).
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { REPO_ROOT } from './cases.mjs'

// ref/ directory names; the build output is keyed by the game name.
const REF_DIR = { poe1: 'apt', poe2: 'ee2' }

export function buildDir (game) {
  return path.join(REPO_ROOT, 'tools/golden/.screens-build', game)
}

export function ensureBuild (game, { rebuild = false } = {}) {
  const out = buildDir(game)
  if (!rebuild && fs.existsSync(path.join(out, 'index.html'))) {
    return out
  }

  const renderer = path.join(REPO_ROOT, 'ref', REF_DIR[game], 'renderer')
  const vite = path.join(renderer, 'node_modules/.bin/vite')
  if (!fs.existsSync(vite)) {
    throw new Error(
      `${vite} is missing; run \`devenv tasks run golden:prepare\` first (it does npm ci in ref/)`
    )
  }

  fs.rmSync(out, { recursive: true, force: true })
  console.log(`[screens] building ${game} renderer -> ${path.relative(REPO_ROOT, out)}`)
  const result = spawnSync(vite, ['build', '--outDir', out, '--emptyOutDir'], {
    cwd: renderer,
    stdio: 'inherit'
  })
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`vite build for ${game} failed with status ${result.status}`)
  }
  if (!fs.existsSync(path.join(out, 'index.html'))) {
    throw new Error(`vite build for ${game} produced no index.html in ${out}`)
  }
  return out
}
