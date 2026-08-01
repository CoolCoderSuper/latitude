import assert from 'node:assert/strict';
import test from 'node:test';

import { LatestRequest } from '../src/server/assets/latest-request.js';

test('starting a request aborts and invalidates the previous request', () => {
  const requests = new LatestRequest();
  const first = requests.begin();
  const second = requests.begin();

  assert.equal(first.controller.signal.aborted, true);
  assert.equal(second.controller.signal.aborted, false);
  assert.equal(requests.isCurrent(first.version), false);
  assert.equal(requests.isCurrent(second.version), true);
});

test('finishing an old request does not disturb the current request', () => {
  const requests = new LatestRequest();
  const first = requests.begin();
  const second = requests.begin();

  requests.finish(first.version);
  assert.equal(requests.isCurrent(second.version), true);
  assert.equal(second.controller.signal.aborted, false);
});
