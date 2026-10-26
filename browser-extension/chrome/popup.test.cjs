const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');

async function openPopup({ os = 'win', shortcut = '', missing = false, readError = false, openError = false, userAgent = 'Chrome/130.0' } = {}) {
  const elements = new Map();
  const tabs = [];
  const events = {};
  let currentShortcut = shortcut;
  function element(selector) {
    if (!elements.has(selector)) elements.set(selector, {
      textContent: '', checked: false, classes: new Set(), listeners: {},
      addEventListener(name, callback) { this.listeners[name] = callback; },
      classList: {
        toggle(name, active) { active ? element(selector).classes.add(name) : element(selector).classes.delete(name); },
        remove(name) { element(selector).classes.delete(name); },
      },
    });
    return elements.get(selector);
  }
  const chrome = {
    runtime: { getPlatformInfo: async () => ({ os }) },
    commands: { getAll: async () => {
      if (readError) throw Error('unavailable');
      return missing ? [] : [{ name: '_execute_action', shortcut: currentShortcut }];
    } },
    tabs: { create: async ({ url }) => { if (openError) throw Error('blocked'); tabs.push(url); } },
    storage: { local: { get: async () => ({ enabled: true, message: '下载就绪' }), set: async () => {} } },
  };
  await vm.runInNewContext(fs.readFileSync(__dirname + '/popup.js', 'utf8'), {
    chrome, navigator: { userAgent }, document: { querySelector: element },
    window: { addEventListener: (name, callback) => { events[name] = callback; } },
  });
  await new Promise(setImmediate);
  return { element, tabs, events, setShortcut(value) { currentShortcut = value; } };
}

for (const [os, expected] of [['win', 'Ctrl + Shift + Y'], ['mac', 'Command + Shift + Y']]) {
  test(`${os}: unassigned shortcut identifies TDM action and opens settings`, async () => {
    const popup = await openPopup({ os });
    const help = popup.element('#shortcut-help');
    assert.match(help.textContent, /TDM 下载助手 · 打开助手/);
    assert.match(help.textContent, /可能被占用或已清除/);
    assert.ok(help.textContent.includes(expected));
    assert.ok(help.classes.has('shortcut-warning'));
    await popup.element('#shortcut-settings').listeners.click();
    assert.deepEqual(popup.tabs, ['chrome://extensions/shortcuts']);
    assert.equal(popup.element('#status').textContent, '下载就绪');
    assert.equal(popup.element('#enabled').checked, true);
  });
}

test('Edge opens its own shortcut settings', async () => {
  const popup = await openPopup({ userAgent: 'Chrome/130.0 Edg/130.0' });
  await popup.element('#shortcut-settings').listeners.click();
  assert.deepEqual(popup.tabs, ['edge://extensions/shortcuts']);
});

test('shows the actual custom binding and clears warning after returning from settings', async () => {
  const popup = await openPopup();
  popup.setShortcut('Ctrl+Shift+U');
  await popup.events.focus();
  assert.match(popup.element('#shortcut-help').textContent, /Ctrl\+Shift\+U/);
  assert.equal(popup.element('#shortcut-help').classes.has('shortcut-warning'), false);
  assert.equal(popup.element('#shortcut-settings').textContent, '修改快捷键');
});

for (const options of [{ missing: true }, { readError: true }]) {
  test(`unavailable command data does not claim a conflict: ${JSON.stringify(options)}`, async () => {
    const popup = await openPopup(options);
    assert.match(popup.element('#shortcut-help').textContent, /无法读取/);
    assert.doesNotMatch(popup.element('#shortcut-help').textContent, /被占用/);
    assert.equal(popup.element('#enabled').checked, true);
  });
}

test('failed navigation explains where to change the TDM shortcut', async () => {
  const popup = await openPopup({ openError: true });
  await popup.element('#shortcut-settings').listeners.click();
  assert.match(popup.element('#shortcut-error').textContent, /chrome:\/\/extensions\/shortcuts/);
  assert.match(popup.element('#shortcut-error').textContent, /TDM 下载助手 → 激活此扩展程序/);
});
