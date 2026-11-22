import path from 'node:path'
import { defineConfig } from 'vitest/config'

// One project per game. Each points the reference's own aliases at its checkout,
// so the sources run unmodified and nothing is written inside ref/.
const REF = {
  poe1: path.resolve(import.meta.dirname, '../../ref/apt/renderer'),
  poe2: path.resolve(import.meta.dirname, '../../ref/ee2/renderer')
}

function project (game: keyof typeof REF) {
  const ref = REF[game]
  return {
    resolve: {
      alias: [
        { find: /^@\//, replacement: `${ref}/src/` },
        { find: /^@ipc\//, replacement: `${ref}/../ipc/` },
        // client_strings.js is a dynamic `import(BASE_URL + 'data/...')`.
        { find: /^\/data\//, replacement: `${ref}/public/data/` }
      ]
    },
    define: { 'import.meta.vitest': 'undefined' },
    test: {
      name: game,
      include: [`src/${game}.test.ts`],
      setupFiles: ['src/setup.ts'],
      env: { GOLDEN_GAME: game, GOLDEN_REF: ref },
      testTimeout: 120_000,
      hookTimeout: 120_000
    }
  }
}

export default defineConfig({
  test: {
    projects: [project('poe1'), project('poe2')]
  }
})
