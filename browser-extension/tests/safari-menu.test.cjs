const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

test('Safari replaces stale menu without corrupting Chinese under legacy script decoding', async () => {
  let installed;
  const calls = [];
  const event = { addListener() {} };
  const browser = {
    runtime: { onInstalled: { addListener(fn) { installed = fn; } }, onMessage: event },
    contextMenus: {
      removeAll: async () => calls.push('remove'),
      create: async (item) => calls.push(item.title),
      onClicked: event,
    },
  };
  const source = fs.readFileSync(path.join(__dirname, '../safari/background.js'));
  vm.runInNewContext(source.toString('latin1'), { browser, console });
  // Updates may arrive while background initialization is still running.
  await Promise.all([installed(), installed()]);
  assert.deepEqual(calls, ['remove', '使用 TDM 下载此链接']);
});
