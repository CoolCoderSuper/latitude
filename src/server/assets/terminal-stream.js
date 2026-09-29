const TERMINAL_OUTPUT_FRAME_KIND = 1;
const TERMINAL_OUTPUT_FRAME_HEADER_BYTES = 17;
const TERMINAL_RESET_SEQUENCE = new Uint8Array([0x1b, 0x63]);

export const decodeTerminalOutputFrame = (data) => {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  if (
    bytes.byteLength < TERMINAL_OUTPUT_FRAME_HEADER_BYTES ||
    bytes[0] !== TERMINAL_OUTPUT_FRAME_KIND
  ) {
    throw new Error('invalid terminal output frame');
  }

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const firstSequence = Number(view.getBigUint64(1, false));
  const endSequence = Number(view.getBigUint64(9, false));
  if (
    !Number.isSafeInteger(firstSequence) ||
    !Number.isSafeInteger(endSequence) ||
    firstSequence > endSequence
  ) {
    throw new Error('invalid terminal output sequence');
  }

  return {
    firstSequence,
    endSequence,
    payload: bytes.subarray(TERMINAL_OUTPUT_FRAME_HEADER_BYTES),
  };
};

export const createTerminalOutputWriter = (
  terminal,
  { sendAck, onProtocolError, onReady } = {},
) => {
  let activeSocket = null;
  let lastSequence = null;
  let ready = false;
  let writing = false;
  let requiresReset = false;
  let frames = [];
  let idleCallbacks = [];

  const notifyIdle = () => {
    if (writing) {
      return;
    }
    const callbacks = idleCallbacks;
    idleCallbacks = [];
    callbacks.forEach((callback) => callback());
  };

  const fail = (message, socket = activeSocket) => {
    if (socket !== activeSocket) {
      return;
    }
    ready = false;
    requiresReset = true;
    frames = [];
    onProtocolError?.(message, socket);
  };

  const drain = () => {
    if (!ready || writing || frames.length === 0) {
      notifyIdle();
      return;
    }

    const frame = frames.shift();
    const expectedSequence = (lastSequence ?? -1) + 1;
    if (frame.firstSequence !== expectedSequence) {
      fail(
        `terminal output sequence jumped from ${lastSequence ?? 'none'} to ${frame.firstSequence}`,
        frame.socket,
      );
      notifyIdle();
      return;
    }

    writing = true;
    try {
      terminal.write(frame.payload, () => {
        writing = false;
        if (requiresReset) {
          lastSequence = null;
        } else {
          lastSequence = frame.endSequence;
          if (activeSocket === frame.socket) {
            sendAck?.(frame.endSequence, frame.socket);
          }
        }
        notifyIdle();
        drain();
      });
    } catch (error) {
      writing = false;
      fail(
        error instanceof Error ? error.message : String(error),
        frame.socket,
      );
      notifyIdle();
    }
  };

  return {
    begin(socket) {
      activeSocket = socket;
      ready = false;
      frames = [];
    },
    end(socket) {
      if (activeSocket !== socket) {
        return;
      }
      activeSocket = null;
      ready = false;
      frames = [];
      notifyIdle();
    },
    whenIdle(callback) {
      if (writing) {
        idleCallbacks.push(callback);
      } else {
        callback();
      }
    },
    lastSequence() {
      return requiresReset ? null : lastSequence;
    },
    acceptReady(message, socket) {
      if (
        activeSocket !== socket ||
        !message ||
        message.type !== 'ready' ||
        typeof message.reset !== 'boolean' ||
        !Number.isSafeInteger(message.sequence) ||
        message.sequence < 0
      ) {
        fail('invalid terminal ready message', socket);
        return;
      }
      if (writing) {
        fail(
          'terminal became ready while output was still being parsed',
          socket,
        );
        return;
      }

      ready = false;
      frames = [];
      if (!message.reset) {
        if (requiresReset || lastSequence !== message.sequence) {
          fail('terminal resume sequence did not match the client', socket);
          return;
        }
        ready = true;
        onReady?.(false, socket);
        drain();
        return;
      }

      writing = true;
      try {
        terminal.write(TERMINAL_RESET_SEQUENCE, () => {
          writing = false;
          if (activeSocket !== socket) {
            requiresReset = false;
            lastSequence = message.sequence;
            notifyIdle();
            return;
          }
          requiresReset = false;
          lastSequence = message.sequence;
          ready = true;
          onReady?.(true, socket);
          notifyIdle();
          drain();
        });
      } catch (error) {
        writing = false;
        fail(error instanceof Error ? error.message : String(error), socket);
        notifyIdle();
      }
    },
    acceptFrame(data, socket) {
      if (activeSocket !== socket) {
        return;
      }
      try {
        frames.push({ ...decodeTerminalOutputFrame(data), socket });
        drain();
      } catch (error) {
        fail(error instanceof Error ? error.message : String(error), socket);
      }
    },
  };
};
