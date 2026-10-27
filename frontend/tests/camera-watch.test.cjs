const { test } = require('node:test');
const assert = require('node:assert/strict');
const { CameraWatch } = require('../.test-build/camera-watch.js');
const frame = (name) => ({ok:true, fresh:true, source_name:name, source_time:'2000-01-01T00:00:00Z'});
test('processes backend updates continuously without client clock comparisons or duplicates', async () => {
  const watcher = new CameraWatch(); let latest = frame('a'); const seen = [];
  const poll = () => watcher.poll(async () => latest, async () => {seen.push(latest.source_name); return latest;}, () => true);
  await poll(); await poll(); latest = frame('b'); await poll();
  assert.deepEqual(seen, ['a', 'b']);
});
test('serializes polls, skips stale photos and pauses before capture when editing starts', async () => {
  const watcher = new CameraWatch(); let release; let captures = 0; let active = true;
  const pending = watcher.poll(() => new Promise(r => release=r), async () => {captures++;}, () => active);
  await watcher.poll(async () => frame('b'), async () => {captures++;}, () => active);
  active=false; release(frame('a')); await pending;
  await watcher.poll(async () => ({...frame('old'),fresh:false}), async () => {captures++;}, () => true);
  assert.equal(captures, 0);
});
test('failed capture retries and a newer captured frame is not recognized twice', async () => {
  const watcher = new CameraWatch(); let calls=0;
  await assert.rejects(watcher.poll(async () => frame('a'), async () => {throw Error('offline');}, () => true));
  await watcher.poll(async () => frame('a'), async () => {calls++; return frame('b');}, () => true);
  await watcher.poll(async () => frame('b'), async () => {calls++;}, () => true);
  assert.equal(calls,1);
});
