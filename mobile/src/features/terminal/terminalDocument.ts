export function terminalDocument(projectName: string, websocketUrl: string): string {
  const projectNameJson = JSON.stringify(projectName);
  const websocketUrlJson = JSON.stringify(websocketUrl);

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css" />
  <style>
    html,
    body {
      height: 100%;
      margin: 0;
      overflow: hidden;
      background: #101514;
      color: #edf4f1;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    body {
      box-sizing: border-box;
      display: grid;
      grid-template-rows: auto minmax(0, 1fr);
      gap: 8px;
      padding: env(safe-area-inset-top, 0) 8px env(safe-area-inset-bottom, 0);
    }

    .bar {
      box-sizing: border-box;
      display: flex;
      align-items: center;
      gap: 8px;
      min-height: 34px;
      border-bottom: 1px solid #2e3936;
      padding: 6px 2px;
      color: #aeb9c7;
      font-size: 12px;
      font-weight: 800;
    }

    .name {
      min-width: 0;
      overflow: hidden;
      color: #edf4f1;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .status {
      margin-left: auto;
      color: #8fe0ad;
      white-space: nowrap;
    }

    .status.error {
      color: #ffb3a7;
    }

    #terminal {
      box-sizing: border-box;
      width: 100%;
      height: 100%;
      min-height: 0;
      overflow: hidden;
      border: 1px solid #2e3936;
      border-radius: 8px;
      padding: 6px;
      background: #101514;
    }

    .xterm {
      height: 100%;
    }

    .xterm .xterm-viewport {
      overflow-y: auto;
    }
  </style>
</head>
<body>
  <div class="bar">
    <span class="name"></span>
    <span class="status">Connecting</span>
  </div>
  <div id="terminal"></div>
  <script src="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js"></script>
  <script>
    const projectName = ${projectNameJson};
    const websocketUrl = ${websocketUrlJson};
    const terminalElement = document.getElementById('terminal');
    const statusElement = document.querySelector('.status');
    document.querySelector('.name').textContent = projectName;

    const setStatus = (text, isError = false) => {
      statusElement.textContent = text;
      statusElement.classList.toggle('error', Boolean(isError));
    };

    const start = () => {
      if (!window.Terminal || !window.FitAddon) {
        setStatus('Assets failed', true);
        return;
      }

      const terminal = new window.Terminal({
        convertEol: true,
        cursorBlink: true,
        cursorStyle: 'block',
        disableStdin: false,
        fontFamily: 'Menlo, Monaco, Consolas, "Liberation Mono", monospace',
        fontSize: 12,
        lineHeight: 1.18,
        scrollback: 5000,
        theme: {
          background: '#101514',
          foreground: '#edf4f1',
          cursor: '#2aa79c',
          selectionBackground: '#2e3936',
          black: '#101514',
          red: '#ff9d87',
          green: '#8fe0ad',
          yellow: '#e1b95a',
          blue: '#9ed2ff',
          magenta: '#c9b6ff',
          cyan: '#73d7e7',
          white: '#edf4f1',
          brightBlack: '#8f9b97',
          brightRed: '#ffd0ca',
          brightGreen: '#c8f2d5',
          brightYellow: '#ffd98b',
          brightBlue: '#c8e4ff',
          brightMagenta: '#e0d6ff',
          brightCyan: '#bdf4fb',
          brightWhite: '#ffffff',
        },
      });
      const fitAddon = new window.FitAddon.FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(terminalElement);
      terminal.focus();

      let socket = null;
      let resizeTimer = null;
      let reconnectTimer = null;
      let reconnectDelay = 1000;
      let hasConnected = false;
      const maxReconnectDelay = 8000;

      const sendJson = (payload) => {
        if (socket && socket.readyState === WebSocket.OPEN) {
          socket.send(JSON.stringify(payload));
        }
      };

      const fitAndResize = () => {
        try {
          fitAddon.fit();
        } catch (_) {
          return;
        }
        sendJson({ type: 'resize', cols: terminal.cols, rows: terminal.rows });
      };

      const socketIsActive = (candidate) =>
        candidate &&
        (candidate.readyState === WebSocket.OPEN ||
          candidate.readyState === WebSocket.CONNECTING);

      const clearReconnectTimer = () => {
        if (reconnectTimer) {
          window.clearTimeout(reconnectTimer);
          reconnectTimer = null;
        }
      };

      const scheduleReconnect = () => {
        clearReconnectTimer();
        const delay = reconnectDelay;
        setStatus('Reconnecting', true);
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null;
          connect();
        }, delay);
        reconnectDelay = Math.min(
          maxReconnectDelay,
          Math.floor(reconnectDelay * 1.6),
        );
      };

      const connect = () => {
        if (socketIsActive(socket)) {
          return;
        }

        clearReconnectTimer();
        setStatus('Connecting');
        const nextSocket = new WebSocket(websocketUrl);
        socket = nextSocket;

        nextSocket.addEventListener('open', () => {
          if (socket !== nextSocket) {
            nextSocket.close();
            return;
          }

          clearReconnectTimer();
          reconnectDelay = 1000;
          if (hasConnected) {
            terminal.reset();
          }
          hasConnected = true;
          setStatus('Connected');
          fitAndResize();
          window.setTimeout(() => setStatus(''), 800);
        });

        nextSocket.addEventListener('message', (event) => {
          if (socket !== nextSocket) {
            return;
          }

          if (typeof event.data === 'string') {
            terminal.write(event.data);
          } else if (event.data instanceof Blob) {
            event.data.text().then((text) => terminal.write(text));
          }
        });

        nextSocket.addEventListener('close', () => {
          if (socket !== nextSocket) {
            return;
          }

          socket = null;
          scheduleReconnect();
        });

        nextSocket.addEventListener('error', () => {
          if (socket !== nextSocket) {
            return;
          }

          setStatus('Connection failed', true);
          try {
            nextSocket.close();
          } catch (_) {
            scheduleReconnect();
          }
        });
      };

      window.latitudeReconnect = (force) => {
        clearReconnectTimer();
        reconnectDelay = 1000;
        if (force && socket && socket.readyState !== WebSocket.CLOSED) {
          const staleSocket = socket;
          socket = null;
          try {
            staleSocket.close();
          } catch (_) {}
        }

        if (socketIsActive(socket)) {
          fitAndResize();
          return;
        }

        connect();
      };

      terminal.onData((data) => {
        if (!socketIsActive(socket)) {
          window.latitudeReconnect(false);
        }
        sendJson({ type: 'input', data });
      });

      const queueResize = () => {
        window.clearTimeout(resizeTimer);
        resizeTimer = window.setTimeout(fitAndResize, 80);
      };

      window.addEventListener('resize', queueResize);
      window.addEventListener('focus', () => window.latitudeReconnect(false));
      window.addEventListener('online', () => window.latitudeReconnect(true));
      window.visualViewport?.addEventListener('resize', queueResize);
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') {
          window.latitudeReconnect(false);
        }
      });
      terminalElement.addEventListener('touchstart', () => terminal.focus(), {
        passive: true,
      });

      connect();
    };

    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', start);
    } else {
      start();
    }
  </script>
</body>
</html>`;
}
