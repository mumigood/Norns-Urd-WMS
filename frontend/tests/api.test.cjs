const { test, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const { api, token, authenticatedImage, Unauthenticated } = require('../.test-build/api.js');
let values;
beforeEach(() => {
  values = new Map();
  global.window = new EventTarget();
  global.localStorage = { getItem: k => values.get(k) ?? null, setItem: (k,v) => values.set(k,v), removeItem: k => values.delete(k) };
});
test('private images use bearer authentication and return binary content', async () => {
  token.set('session');
  global.fetch = async (url, options) => {
    assert.equal(url, '/uploads/photo.jpg');
    assert.equal(options.headers.Authorization, 'Bearer session');
    return new Response(new Blob(['photo'], {type:'image/jpeg'}));
  };
  assert.equal(typeof authenticatedImage, 'function');
  assert.equal(await (await authenticatedImage('/uploads/photo.jpg')).text(), 'photo');
});
test('private image loader never sends credentials to another origin', async () => {
  global.fetch = () => assert.fail('must not fetch an external image');
  assert.equal(typeof authenticatedImage, 'function');
  await assert.rejects(() => authenticatedImage('https://other.example/uploads/photo.jpg'));
});
test('expired session clears local credentials and notifies the login gate', async () => {
  token.set('expired');
  let expired = false;
  window.addEventListener('urdwms:unauthenticated', () => { expired = true; });
  global.fetch = async () => new Response(JSON.stringify({detail:'請先登入'}), {status:401});
  await assert.rejects(api.me, Unauthenticated);
  assert.equal(token.get(), null);
  assert.equal(expired, true);
});
