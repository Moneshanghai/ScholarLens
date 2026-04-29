import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

function lastZIndexForSelector(css, selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const blockRe = new RegExp(`${escaped}\\s*\\{([^}]*)\\}`, 'g');
  let zIndex = null;
  for (const match of css.matchAll(blockRe)) {
    const z = match[1].match(/z-index\s*:\s*([0-9]+)/);
    if (z) zIndex = Number(z[1]);
  }
  return zIndex;
}

test('history sidebar stays above overlay so history items are clickable', () => {
  const css = readFileSync(new URL('../styles/index.css', import.meta.url), 'utf8');
  const sidebarZ = lastZIndexForSelector(css, '.sidebar');
  const overlayZ = lastZIndexForSelector(css, '.sidebar-overlay');

  assert.ok(Number.isFinite(sidebarZ), 'sidebar z-index should be declared');
  assert.ok(Number.isFinite(overlayZ), 'overlay z-index should be declared');
  assert.ok(
    sidebarZ > overlayZ,
    `sidebar z-index (${sidebarZ}) must be above overlay z-index (${overlayZ})`
  );
});
