import assert from 'node:assert/strict';
import test from 'node:test';

import {
  createTerminalOutputWriter,
  decodeTerminalOutputFrame,
} from '../src/server/assets/terminal-stream.js';

const terminalFrame = (firstSequence, endSequence, text) => {
  const payload = new TextEncoder().encode(text);
  const frame = new Uint8Array(17 + payload.length);
  const view = new DataView(frame.buffer);
  frame[0] = 1;
  view.setBigUint64(1, BigInt(firstSequence), false);
  view.setBigUint64(9, BigInt(endSequence), false);
  frame.set(payload, 17);
  return frame;
};

class FakeTerminal {
  constructor() {
    this.writes = [];
    this.callbacks = [];
  }

  write(data, callback) {
    this.writes.push(new Uint8Array(data));
    this.callbacks.push(callback);
  }

  flush() {
    const callback = this.callbacks.shift();
    assert.ok(callback, 'expected a pending terminal write');
    callback();
  }
}

test('decodes sequenced terminal output frames', () => {
  const decoded = decodeTerminalOutputFrame(terminalFrame(7, 9, 'hello'));

  assert.equal(decoded.firstSequence, 7);
  assert.equal(decoded.endSequence, 9);
  assert.equal(new TextDecoder().decode(decoded.payload), 'hello');
});

test('hard reset and output are parsed in order before acknowledgement', () => {
  const terminal = new FakeTerminal();
  const socket = {};
  const acknowledgements = [];
  const writer = createTerminalOutputWriter(terminal, {
    sendAck: (sequence) => acknowledgements.push(sequence),
  });

  writer.begin(socket);
  writer.acceptReady({ type: 'ready', reset: true, sequence: 6 }, socket);
  writer.acceptFrame(terminalFrame(7, 8, 'screen'), socket);

  assert.deepEqual([...terminal.writes[0]], [0x1b, 0x63]);
  assert.equal(terminal.writes.length, 1);
  terminal.flush();
  assert.equal(new TextDecoder().decode(terminal.writes[1]), 'screen');
  assert.deepEqual(acknowledgements, []);
  terminal.flush();
  assert.deepEqual(acknowledgements, [8]);
  assert.equal(writer.lastSequence(), 8);
});

test('reconnect hello waits for an in-progress parser write', () => {
  const terminal = new FakeTerminal();
  const oldSocket = {};
  const nextSocket = {};
  const idleSequences = [];
  const acknowledgements = [];
  const writer = createTerminalOutputWriter(terminal, {
    sendAck: (sequence) => acknowledgements.push(sequence),
  });

  writer.begin(oldSocket);
  writer.acceptReady({ type: 'ready', reset: true, sequence: 0 }, oldSocket);
  terminal.flush();
  writer.acceptFrame(terminalFrame(1, 1, 'delta'), oldSocket);
  writer.end(oldSocket);
  writer.begin(nextSocket);
  writer.whenIdle(() => idleSequences.push(writer.lastSequence()));

  assert.deepEqual(idleSequences, []);
  terminal.flush();
  assert.deepEqual(idleSequences, [1]);
  assert.deepEqual(acknowledgements, []);
});
