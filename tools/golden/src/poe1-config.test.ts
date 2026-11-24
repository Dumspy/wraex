// PoE1 settings: ref/apt/renderer/src/web/Config.ts (see config-cases.ts).
// The widgets first, as the app's App.vue does: they and Config.ts import each
// other, and EE2's widgets read Config's exports while loading.
import '@/web/overlay/widget-registry'
import * as config from '@/web/Config'
import { configCases } from './config-cases'

configCases('poe1', config as never)
