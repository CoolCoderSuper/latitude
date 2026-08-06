import type { ThemeColors, ThemeMode } from '../../theme';

type XtermTheme = {
  background: string;
  foreground: string;
  cursor: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
};

type TerminalDocumentTheme = {
  mode: ThemeMode;
  variables: Record<string, string>;
  xterm: XtermTheme;
};

export function terminalDocumentTheme(
  mode: ThemeMode,
  colors: ThemeColors,
): TerminalDocumentTheme {
  return {
    mode,
    variables: {
      '--terminal-page-bg': colors.background,
      '--terminal-page-text': colors.text,
      '--terminal-heading': colors.text,
      '--terminal-muted': colors.muted,
      '--terminal-accent': colors.accent,
      '--terminal-border': colors.border,
      '--terminal-status-text': colors.success,
      '--terminal-error-text': colors.danger,
      '--terminal-xterm-bg':
        mode === 'dark' ? colors.background : colors.codeBg,
      '--terminal-xterm-fg': colors.codeText,
    },
    xterm:
      mode === 'dark'
        ? {
            background: colors.background,
            foreground: colors.text,
            cursor: colors.accent,
            selectionBackground: colors.border,
            black: colors.background,
            red: colors.coral,
            green: colors.success,
            yellow: colors.gold,
            blue: '#9ed2ff',
            magenta: colors.tokenKeyword,
            cyan: colors.tokenNumber,
            white: colors.text,
            brightBlack: colors.muted,
            brightRed: colors.codeRemoveText,
            brightGreen: colors.codeAddText,
            brightYellow: colors.tokenString,
            brightBlue: '#c8e4ff',
            brightMagenta: '#e0d6ff',
            brightCyan: '#bdf4fb',
            brightWhite: '#ffffff',
          }
        : {
            background: colors.codeBg,
            foreground: colors.codeText,
            cursor: colors.accent,
            selectionBackground: '#cfe9e4',
            black: '#111827',
            red: colors.danger,
            green: colors.accent,
            yellow: colors.gold,
            blue: colors.tokenType,
            magenta: colors.tokenKeyword,
            cyan: colors.tokenNumber,
            white: '#f8fafc',
            brightBlack: colors.codeMuted,
            brightRed: '#dc2626',
            brightGreen: '#16a34a',
            brightYellow: '#ca8a04',
            brightBlue: '#2563eb',
            brightMagenta: '#9333ea',
            brightCyan: '#0891b2',
            brightWhite: '#ffffff',
          },
  };
}

export function terminalThemeInjectionScript(
  theme: TerminalDocumentTheme,
): string {
  const themeJson = JSON.stringify(theme);

  return `
(function() {
  window.__latitudeTerminalTheme = ${themeJson};
  if (window.latitudeSetTerminalTheme) {
    window.latitudeSetTerminalTheme(window.__latitudeTerminalTheme);
  }
})();
true;
`;
}

export function terminalDocument(
  projectName: string,
  websocketUrl: string,
  theme: TerminalDocumentTheme,
  serverBaseUrl: string,
): string {
  const projectNameJson = JSON.stringify(projectName);
  const websocketUrlJson = JSON.stringify(websocketUrl);
  const themeJson = JSON.stringify(theme);
  const cssVariables = terminalCssVariables(theme);
  const terminalStyleUrl = htmlAttribute(
    new URL(
      '/__latitude/assets/terminal-viewer.bundle.css',
      serverBaseUrl,
    ).toString(),
  );
  const terminalScriptUrl = htmlAttribute(
    new URL(
      '/__latitude/assets/terminal-viewer.bundle.js',
      serverBaseUrl,
    ).toString(),
  );

  return `<!doctype html>
<html lang="en" data-latitude-theme="${theme.mode}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <link rel="stylesheet" href="${terminalStyleUrl}" />
  <style>
    :root {
      color-scheme: ${theme.mode};
${cssVariables}
      background: var(--terminal-page-bg);
      color: var(--terminal-page-text);
    }

    html,
    body {
      height: 100%;
      margin: 0;
      overflow: hidden;
      background: var(--terminal-page-bg);
      color: var(--terminal-page-text);
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
      border-bottom: 1px solid var(--terminal-border);
      padding: 6px 2px;
      color: var(--terminal-muted);
      font-size: 12px;
      font-weight: 800;
    }

    .name {
      min-width: 0;
      overflow: hidden;
      color: var(--terminal-heading);
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .status {
      margin-left: auto;
      color: var(--terminal-status-text);
      white-space: nowrap;
    }

    .status.error {
      color: var(--terminal-error-text);
    }

    #terminal {
      box-sizing: border-box;
      width: 100%;
      height: 100%;
      min-height: 0;
      overflow: hidden;
      border: 1px solid var(--terminal-border);
      border-radius: 8px;
      padding: 6px;
      background: var(--terminal-xterm-bg);
    }

    .xterm {
      height: 100%;
    }

    .xterm .xterm-viewport {
      background: var(--terminal-xterm-bg);
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
  <script type="module" src="${terminalScriptUrl}"></script>
  <script>
    const projectName = ${projectNameJson};
    const websocketUrl = ${websocketUrlJson};
    const initialTheme = window.__latitudeTerminalTheme || ${themeJson};
    const terminalElement = document.getElementById('terminal');
    const statusElement = document.querySelector('.status');
    let currentTheme = initialTheme;
    let terminal = null;
    document.querySelector('.name').textContent = projectName;

    const applyCssVariables = (theme) => {
      if (!theme || !theme.variables) {
        return;
      }

      const root = document.documentElement;
      root.dataset.latitudeTheme = theme.mode;
      root.style.colorScheme = theme.mode;
      Object.keys(theme.variables).forEach((name) => {
        root.style.setProperty(name, theme.variables[name]);
      });
    };

    const applyTerminalTheme = (nextTerminal, theme) => {
      if (!nextTerminal || !theme || !theme.xterm) {
        return;
      }

      if (nextTerminal.options) {
        nextTerminal.options.theme = theme.xterm;
      } else if (typeof nextTerminal.setOption === 'function') {
        nextTerminal.setOption('theme', theme.xterm);
      }
      if (typeof nextTerminal.refresh === 'function') {
        nextTerminal.refresh(0, Math.max(0, nextTerminal.rows - 1));
      }
    };

    window.latitudeSetTerminalTheme = (theme) => {
      currentTheme = theme || currentTheme;
      applyCssVariables(currentTheme);
      applyTerminalTheme(terminal, currentTheme);
    };

    window.latitudeSetTerminalTheme(currentTheme);

    const setStatus = (text, isError = false) => {
      statusElement.textContent = text;
      statusElement.classList.toggle('error', Boolean(isError));
    };

    const start = () => {
      if (!window.Terminal || !window.FitAddon) {
        setStatus('Assets failed', true);
        return;
      }

      terminal = new window.Terminal({
        convertEol: false,
        cursorBlink: true,
        cursorStyle: 'block',
        disableStdin: false,
        fontFamily: 'Consolas, "Cascadia Mono", "DejaVu Sans Mono", monospace',
        fontSize: 14,
        lineHeight: 1,
        letterSpacing: 0,
        scrollback: 5000,
        theme: currentTheme.xterm,
      });
      applyTerminalTheme(terminal, currentTheme);
      const fitAddon = new window.FitAddon.FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(terminalElement);
      let webglAddon = null;
      if (window.WebglAddon && window.WebglAddon.WebglAddon) {
        try {
          webglAddon = new window.WebglAddon.WebglAddon();
          webglAddon.onContextLoss(() => webglAddon.dispose());
          terminal.loadAddon(webglAddon);
        } catch (_) {
          webglAddon?.dispose();
        }
      }
      terminal.focus();

      let socket = null;
      let resizeTimer = null;
      let resizeObserver = null;
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
          const terminalRect = terminalElement.getBoundingClientRect();
          const terminalStyles = window.getComputedStyle(terminalElement);
          const contentBottom =
            terminalRect.bottom -
            (Number.parseFloat(terminalStyles.paddingBottom) || 0);
          for (let attempt = 0; attempt < 3; attempt += 1) {
            const screen = terminalElement.querySelector('.xterm-screen');
            if (
              !screen ||
              screen.getBoundingClientRect().bottom <= contentBottom + 0.5 ||
              terminal.rows <= 2
            ) {
              break;
            }
            terminal.resize(terminal.cols, terminal.rows - 1);
          }
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

      const scheduleReconnect = (resynchronize = false) => {
        clearReconnectTimer();
        const delay = reconnectDelay;
        setStatus(resynchronize ? 'Resynchronizing' : 'Reconnecting', true);
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
        nextSocket.binaryType = 'arraybuffer';
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
          } else if (event.data instanceof ArrayBuffer) {
            terminal.write(new Uint8Array(event.data));
          } else if (event.data instanceof Blob) {
            event.data.arrayBuffer().then((buffer) => {
              if (socket === nextSocket) {
                terminal.write(new Uint8Array(buffer));
              }
            });
          }
        });

        nextSocket.addEventListener('close', (event) => {
          if (socket !== nextSocket) {
            return;
          }

          socket = null;
          scheduleReconnect(event.code === 1013);
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

      if (typeof ResizeObserver === 'function') {
        resizeObserver = new ResizeObserver(queueResize);
        resizeObserver.observe(terminalElement);
      }
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
      window.addEventListener('beforeunload', () => {
        resizeObserver?.disconnect();
        window.clearTimeout(resizeTimer);
        clearReconnectTimer();
        if (socket) {
          const staleSocket = socket;
          socket = null;
          try {
            staleSocket.close();
          } catch (_) {}
        }
        terminal.dispose();
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

function terminalCssVariables(theme: TerminalDocumentTheme): string {
  return Object.entries(theme.variables)
    .map(([name, value]) => `      ${name}: ${value};`)
    .join('\n');
}

function htmlAttribute(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
}
