#!/usr/bin/env python3
# Usage: python3 tools/golden/scripts/make-config-corpus.py <repo root>, after a regen.
# Derives the settings corpus from the recorded defaults (fixtures/config/*/default/config.json):
# old versions and field names set by hand, per the references' upgradeConfig steps.
import copy, json, sys
root = sys.argv[1]
def load(g): return json.load(open(f'{root}/fixtures/config/{g}/default/config.json'))
def w(cfg, t, n=0): return [x for x in cfg['widgets'] if x['wmType'] == t][n]
def save(g, slug, cfg):
    with open(f'{root}/tools/golden/corpus/config/{g}/{slug}.json', 'w') as f:
        f.write(json.dumps(cfg, indent=2) + '\n')

# PoE1, version 15: hotkeys at the top level, old menu flag, fields added since.
c = load('poe1')
c['configVersion'] = 15
c['delveGridKey'] = 'Ctrl + G'; del w(c, 'delve-grid')['toggleKey']
c['itemCheckKey'] = 'Ctrl + I'; del w(c, 'item-check')['hotkey']
w(c, 'delve-grid')['wmFlags'] = ['hide-on-focus', 'skip-menu']
del w(c, 'stash-search', 1)['enableHotkeys']
pc = w(c, 'price-check'); del pc['merchantOnly']; del pc['defaultCurrency']
del c['logKeys']; del c['useIntlSite']
c['language'] = 'cmn-Hant'; c['realm'] = 'pc-ggg'
save('poe1', 'v15-legacy', c)

# PoE1, current, with every kind of shortcut set.
c = load('poe1')
c['commands'][2]['hotkey'] = 'Ctrl + F2'
ic = w(c, 'item-check'); ic.update(wikiKey='F8', craftOfExileKey='Ctrl + Shift + C', poedbKey='F7', stashSearchKey='Ctrl + F', hotkey='Ctrl + I')
w(c, 'delve-grid')['toggleKey'] = 'Ctrl + G'
w(c, 'item-search')['ocrGemsKey'] = 'Ctrl + O'
w(c, 'stash-search', 0)['entries'][0]['hotkey'] = 'Ctrl + 1'
s2 = w(c, 'stash-search', 1); s2['enableHotkeys'] = False; s2['entries'][0]['hotkey'] = 'Ctrl + 2'
save('poe1', 'custom-hotkeys', c)

# PoE2, version 28: before the experience tracker, library and later price-check fields.
c = load('poe2')
c['configVersion'] = 28
c['widgets'] = [x for x in c['widgets'] if x['wmType'] not in ('experience-tracker', 'library')]
pc = w(c, 'price-check')
for k in ('coreCurrency', 'currencyVolume', 'rememberListingType', 'initialDelay', 'savedAugments'):
    pc.pop(k, None)
for k in ('readClientLog', 'hideOverlayOnBlur'):
    c.pop(k, None)
c['tipsFrequency'] = 1
save('poe2', 'v28-legacy', c)

# PoE2, current, with shortcuts set.
c = load('poe2')
c['commands'][2]['hotkey'] = 'Ctrl + F2'
ic = w(c, 'item-check'); ic.update(wikiKey='F8', craftOfExileKey='Ctrl + Shift + C', poedbKey='F7', stashSearchKey='Ctrl + F', hotkey='Ctrl + I', samePricedKey='Ctrl + P')
w(c, 'delve-grid')['toggleKey'] = 'Ctrl + G'
w(c, 'stash-search', 0)['entries'][0]['hotkey'] = 'Ctrl + 1'
save('poe2', 'custom-hotkeys', c)

for g in ('poe1', 'poe2'):
    with open(f'{root}/tools/golden/corpus/config/{g}/unreadable.json', 'w') as f:
        f.write('{"configVersion": 18, "widgets": [\n')
