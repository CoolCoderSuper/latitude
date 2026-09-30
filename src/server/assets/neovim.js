import { NeovimGrid, keyInput } from './neovim-grid.js';
import { editorFontFamily, editorFontSize } from './editor-font.js';
import { drawBoxDrawing } from './neovim-box-drawing.js';

const root = document.querySelector('[data-neovim]');
if (root) {
  const editor = root.querySelector('[data-neovim-editor]');
  const canvas = root.querySelector('canvas');
  const input = root.querySelector('textarea');
  const status = root.querySelector('[data-neovim-status]');
  const notice = root.querySelector('[data-neovim-notice]');
  const dismiss = root.querySelector('[data-neovim-dismiss]');
  const restart = root.querySelector('[data-neovim-restart]');
  const ctx = canvas.getContext('2d');
  const font = `${editorFontSize}px ${editorFontFamily}`;
  await document.fonts.load(font);
  ctx.font = font;
  const cellWidth = ctx.measureText('M').width;
  const cellHeight = 20;
  let grid;
  let socket;
  let ready = false;
  let frame;
  let lastSize = '';
  let pressedButton;
  let leaving = false;
  let reconnectTimer;
  let requestedPath = new URLSearchParams(location.search).get('path');

  function send(value) {
    if (ready && socket?.readyState === WebSocket.OPEN)
      socket.send(JSON.stringify(value));
  }

  function resize() {
    const width = Math.max(
      2,
      Math.min(500, Math.floor(editor.clientWidth / cellWidth)),
    );
    const height = Math.max(
      2,
      Math.min(300, Math.floor(editor.clientHeight / cellHeight)),
    );
    const size = `${width}:${height}`;
    if (ready && size !== lastSize) {
      send({ type: 'resize', width, height });
      lastSize = size;
    }
    scheduleDraw();
  }

  function scheduleDraw() {
    if (!frame) frame = requestAnimationFrame(draw);
  }

  function draw() {
    frame = null;
    const scale = window.devicePixelRatio || 1;
    const width = editor.clientWidth;
    const height = editor.clientHeight;
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
    ctx.scale(scale, scale);
    ctx.fillStyle = grid.background;
    ctx.fillRect(0, 0, width, height);
    ctx.textBaseline = 'alphabetic';
    // Paint backgrounds first so the continuation cell of a wide character
    // does not erase the right half of its glyph.
    for (let row = 0; row < grid.height; row++) {
      for (let col = 0; col < grid.width; col++) {
        ctx.fillStyle = grid.style(grid.cells[row][col][1]).background;
        ctx.fillRect(
          col * cellWidth,
          row * cellHeight,
          cellWidth + 0.5,
          cellHeight,
        );
      }
    }
    function glyph(row, col, override) {
      const cell = grid.cells[row]?.[col];
      if (!cell) return;
      const attrs = override ?? grid.style(cell[1]);
      ctx.font = `${attrs.italic ? 'italic ' : ''}${attrs.bold ? 'bold ' : ''}${font}`;
      ctx.fillStyle = attrs.foreground;
      if (
        !drawBoxDrawing(
          ctx,
          cell[0],
          col * cellWidth,
          row * cellHeight,
          cellWidth,
          cellHeight,
          scale,
        )
      )
        ctx.fillText(cell[0], col * cellWidth, row * cellHeight + 16);
      ctx.fillStyle = attrs.special;
      if (
        attrs.underline ||
        attrs.undercurl ||
        attrs.underdouble ||
        attrs.underdotted ||
        attrs.underdashed
      )
        ctx.fillRect(col * cellWidth, (row + 1) * cellHeight - 2, cellWidth, 1);
      if (attrs.strikethrough)
        ctx.fillRect(col * cellWidth, row * cellHeight + 10, cellWidth, 1);
    }
    for (let row = 0; row < grid.height; row++)
      for (let col = 0; col < grid.width; col++) glyph(row, col);
    const [row, col] = grid.cursor;
    const x = col * cellWidth;
    const y = row * cellHeight;
    if (ready && !grid.busy && document.activeElement === input) {
      const attrs = grid.mode.attr_id
        ? grid.style(grid.mode.attr_id)
        : {
            background: grid.foreground,
            foreground: grid.background,
          };
      ctx.fillStyle = attrs.background;
      const fraction = (grid.mode.cell_percentage || 20) / 100;
      if (grid.mode.cursor_shape === 'vertical')
        ctx.fillRect(x, y, Math.max(2, cellWidth * fraction), cellHeight);
      else if (grid.mode.cursor_shape === 'horizontal')
        ctx.fillRect(
          x,
          y + cellHeight * (1 - fraction),
          cellWidth,
          cellHeight * fraction,
        );
      else {
        ctx.fillRect(x, y, cellWidth, cellHeight);
        glyph(row, col, attrs);
      }
    }
    input.style.left = `${Math.min(x, width - cellWidth)}px`;
    input.style.top = `${Math.min(y, height - cellHeight)}px`;
  }

  function connect() {
    grid = new NeovimGrid();
    ready = false;
    clearTimeout(reconnectTimer);
    lastSize = '';
    restart.hidden = true;
    dismiss.hidden = true;
    notice.hidden = false;
    status.textContent = 'Connecting to Neovim…';
    const url = new URL(root.dataset.wsPath, location.href);
    url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    socket = new WebSocket(url);
    let failed = false;
    let ended = false;
    let detached = false;
    socket.addEventListener('message', ({ data }) => {
      const message = JSON.parse(data);
      if (message.type === 'ready') {
        ready = true;
        notice.hidden = true;
        status.textContent = 'Connected';
        resize();
        if (requestedPath) send({ type: 'open_file', path: requestedPath });
        input.focus({ preventScroll: true });
      } else if (message.type === 'redraw') {
        for (const [name, ...calls] of message.events) {
          for (const args of calls) {
            grid.apply(name, args);
            if (name === 'flush') scheduleDraw();
          }
        }
      } else if (message.type === 'file_opened') {
        requestedPath = null;
      } else if (message.type === 'open_error') {
        requestedPath = null;
        notice.hidden = false;
        dismiss.hidden = false;
        status.textContent = message.message;
      } else if (message.type === 'error') {
        failed = true;
        notice.hidden = false;
        status.textContent = message.message;
      } else if (message.type === 'exit') {
        ended = true;
      } else if (message.type === 'detached') {
        detached = true;
        notice.hidden = false;
        status.textContent = message.message;
      }
    });
    socket.addEventListener('error', () => {
      failed = true;
      status.textContent =
        'Could not connect to Neovim. Check your connection and sign in again if needed.';
    });
    socket.addEventListener('close', () => {
      ready = false;
      notice.hidden = false;
      dismiss.hidden = true;
      if (!failed && !detached)
        status.textContent = ended
          ? 'Neovim session ended.'
          : 'Connection lost. Your session is still running.';
      restart.textContent = ended ? 'Start new session' : 'Reconnect';
      restart.hidden = false;
      if (!ended && !failed && !detached && !leaving)
        reconnectTimer = setTimeout(connect, 1500);
      scheduleDraw();
    });
    scheduleDraw();
  }

  function textInput() {
    if (input.value)
      send({ type: 'input', keys: input.value.replaceAll('<', '<LT>') });
    input.value = '';
  }
  input.addEventListener('input', (event) => {
    if (!event.isComposing) textInput();
  });
  input.addEventListener('compositionend', textInput);
  input.addEventListener('keydown', (event) => {
    const keys = keyInput(event);
    if (keys) {
      event.preventDefault();
      send({ type: 'input', keys });
    }
  });
  input.addEventListener('paste', (event) => {
    event.preventDefault();
    const text = event.clipboardData.getData('text/plain');
    if (new TextEncoder().encode(text).length > 512 * 1024) {
      notice.hidden = false;
      dismiss.hidden = false;
      status.textContent =
        'Paste is limited to 512 KiB. Open larger files from Neovim.';
      return;
    }
    send({ type: 'paste', text });
  });
  input.addEventListener('focus', scheduleDraw);
  input.addEventListener('blur', scheduleDraw);
  function mouse(event, button, action) {
    const rect = canvas.getBoundingClientRect();
    send({
      type: 'mouse',
      button,
      action,
      modifiers: `${event.shiftKey ? 'S' : ''}${event.ctrlKey ? 'C' : ''}${event.altKey ? 'A' : ''}${event.metaKey ? 'M' : ''}`,
      row: Math.max(
        0,
        Math.min(
          grid.height - 1,
          Math.floor((event.clientY - rect.top) / cellHeight),
        ),
      ),
      col: Math.max(
        0,
        Math.min(
          grid.width - 1,
          Math.floor((event.clientX - rect.left) / cellWidth),
        ),
      ),
    });
  }
  editor.addEventListener('pointerdown', (event) => {
    if (event.target.closest('[data-neovim-notice]')) return;
    event.preventDefault();
    input.focus({ preventScroll: true });
    pressedButton = ['left', 'middle', 'right'][event.button];
    if (!pressedButton) return;
    editor.setPointerCapture(event.pointerId);
    mouse(event, pressedButton, 'press');
  });
  editor.addEventListener('pointermove', (event) => {
    if (pressedButton) mouse(event, pressedButton, 'drag');
  });
  for (const name of ['pointerup', 'pointercancel'])
    editor.addEventListener(name, (event) => {
      if (pressedButton) mouse(event, pressedButton, 'release');
      pressedButton = null;
    });
  editor.addEventListener('contextmenu', (event) => event.preventDefault());
  editor.addEventListener(
    'wheel',
    (event) => {
      event.preventDefault();
      if (event.deltaY) mouse(event, 'wheel', event.deltaY > 0 ? 'down' : 'up');
      else if (event.deltaX)
        mouse(event, 'wheel', event.deltaX > 0 ? 'right' : 'left');
    },
    { passive: false },
  );
  window.addEventListener('pagehide', () => {
    leaving = true;
    clearTimeout(reconnectTimer);
    socket?.close();
  });
  window.addEventListener('pageshow', (event) => {
    if (event.persisted) {
      leaving = false;
      connect();
    }
  });
  window.addEventListener('resize', resize);
  new ResizeObserver(resize).observe(editor);
  restart.addEventListener('click', connect);
  dismiss.addEventListener('click', () => {
    notice.hidden = true;
    input.focus({ preventScroll: true });
  });
  connect();
}
