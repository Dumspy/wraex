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

// The settings (`web/Config.ts`): unlike the item pipeline, the real module is
// loaded, which imports every widget's .vue file. Each reference's own Vue
// plugin compiles them; happy-dom stands in for the browser they expect.
async function configProject (game: keyof typeof REF) {
  const ref = REF[game]
  const vue = (await import(`${ref}/node_modules/@vitejs/plugin-vue/dist/index.mjs`)).default
  return {
    plugins: [vue()],
    resolve: {
      alias: [
        { find: /^@\//, replacement: `${ref}/src/` },
        { find: /^@ipc\//, replacement: `${ref}/../ipc/` }
      ]
    },
    define: { 'import.meta.vitest': 'undefined' },
    test: {
      name: `${game}-config`,
      include: [`src/${game}-config.test.ts`],
      setupFiles: ['src/setup-config.ts'],
      environment: 'happy-dom',
      env: { GOLDEN_GAME: game, GOLDEN_REF: ref },
      testTimeout: 120_000
    }
  }
}

export default defineConfig({
  test: {
    projects: [project('poe1'), project('poe2'), await configProject('poe1'), await configProject('poe2')]
  }
})
