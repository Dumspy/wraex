// Seeds corpus/poe2 with the item texts of ref/ee2/renderer/specs/Parser/items.ts
// (`export const Name = new TestItem(`...`)`). Existing corpus files are kept, so
// re-running only adds new items.
import fs from 'node:fs'
import path from 'node:path'

const here = import.meta.dirname
const source = path.resolve(here, '../../../ref/ee2/renderer/specs/Parser/items.ts')
const out = path.resolve(here, '../corpus/poe2')

const text = fs.readFileSync(source, 'utf8')
const re = /export const (\w+) = new TestItem\(`([^`]*)`\)/g
let added = 0
let total = 0
for (const [, name, body] of text.matchAll(re)) {
  total += 1
  const slug = 'ee2-' + name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()
  const file = path.join(out, `${slug}.txt`)
  if (fs.existsSync(file)) continue
  fs.writeFileSync(file, body)
  added += 1
}
console.log(`seed-poe2: ${total} items in items.ts, ${added} added to corpus/poe2`)
