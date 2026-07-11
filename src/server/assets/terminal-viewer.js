const workspace = document.querySelector('[data-terminal-workspace]');

if (workspace) {
  const sessionList = workspace.querySelector('[data-terminal-sessions]');
  const newButton = workspace.querySelector('[data-terminal-new]');
  const stack = workspace.querySelector('[data-terminal-stack]');
  const status = workspace.querySelector('[data-terminal-status]');
  const empty = workspace.querySelector('[data-terminal-empty]');
  const terminalControllers = new Map();
  const closingSessions = new Set();
  let activeSessionId = null;
  let creatingSession = false;
  let sessions = [];
  let statusTimer = null;

  const showStatus = (message, isError, autoHide) => {
    if (!status) {
      return;
    }

    window.clearTimeout(statusTimer);
    if (!message) {
      status.hidden = true;
      status.textContent = '';
      status.classList.remove('error');
      return;
    }

    status.hidden = false;
    status.textContent = message;
    status.classList.toggle('error', Boolean(isError));
    if (autoHide) {
      statusTimer = window.setTimeout(() => {
        if (!status.classList.contains('error')) {
          showStatus('', false, false);
        }
      }, 1100);
    }
  };

  const sessionsUrl = () =>
    new URL(workspace.dataset.sessionsPath || '/__latitude/api/projects/terminal/sessions', window.location.href);

  const sessionUrl = (sessionId) => {
    const url = sessionsUrl();
    url.pathname = `${url.pathname.replace(/\/$/, '')}/${encodeURIComponent(sessionId)}`;
    return url;
  };

  const buildSocketUrl = (sessionId) => {
    const url = new URL(workspace.dataset.wsPath || `${window.location.pathname.replace(/\/$/, '')}/ws`, window.location.href);
    url.protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    if (workspace.dataset.wsToken) {
      url.searchParams.set('token', workspace.dataset.wsToken);
    }
    if (sessionId) {
      url.searchParams.set('session', sessionId);
    }
    return url;
  };

  const apiFetch = async (url, options = {}) => {
    const headers = new Headers(options.headers || {});
    headers.set('Accept', 'application/json');
    if (workspace.dataset.wsToken) {
      headers.set('Authorization', `Bearer ${workspace.dataset.wsToken}`);
    }

    const response = await fetch(url, {
      ...options,
      credentials: 'same-origin',
      headers,
    });

    if (!response.ok) {
      let message = `${response.status} ${response.statusText}`.trim();
      try {
        const payload = await response.json();
        message = payload.error || payload.message || message;
      } catch (_) {}
      throw new Error(message);
    }

    if (response.status === 204) {
      return null;
    }

    return response.json();
  };

  const sendJson = (socket, payload) => {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(payload));
    }
  };

  const socketIsActive = (socket) =>
    socket &&
    (socket.readyState === WebSocket.OPEN ||
      socket.readyState === WebSocket.CONNECTING);

  const terminalCssValue = (styles, name, fallback) =>
    styles.getPropertyValue(name).trim() || fallback;

  const terminalTheme = () => {
    const styles = window.getComputedStyle(workspace);
    return {
      background: terminalCssValue(styles, '--terminal-xterm-bg', '#101514'),
      foreground: terminalCssValue(styles, '--terminal-xterm-fg', '#edf4f1'),
      cursor: terminalCssValue(styles, '--terminal-xterm-cursor', '#2aa79c'),
      selectionBackground: terminalCssValue(styles, '--terminal-xterm-selection', '#2e3936'),
      black: terminalCssValue(styles, '--terminal-xterm-black', '#101514'),
      red: terminalCssValue(styles, '--terminal-xterm-red', '#ff9d87'),
      green: terminalCssValue(styles, '--terminal-xterm-green', '#8fe0ad'),
      yellow: terminalCssValue(styles, '--terminal-xterm-yellow', '#e1b95a'),
      blue: terminalCssValue(styles, '--terminal-xterm-blue', '#9ed2ff'),
      magenta: terminalCssValue(styles, '--terminal-xterm-magenta', '#c9b6ff'),
      cyan: terminalCssValue(styles, '--terminal-xterm-cyan', '#73d7e7'),
      white: terminalCssValue(styles, '--terminal-xterm-white', '#edf4f1'),
      brightBlack: terminalCssValue(styles, '--terminal-xterm-bright-black', '#8f9b97'),
      brightRed: terminalCssValue(styles, '--terminal-xterm-bright-red', '#ffd0ca'),
      brightGreen: terminalCssValue(styles, '--terminal-xterm-bright-green', '#c8f2d5'),
      brightYellow: terminalCssValue(styles, '--terminal-xterm-bright-yellow', '#ffd98b'),
      brightBlue: terminalCssValue(styles, '--terminal-xterm-bright-blue', '#c8e4ff'),
      brightMagenta: terminalCssValue(styles, '--terminal-xterm-bright-magenta', '#e0d6ff'),
      brightCyan: terminalCssValue(styles, '--terminal-xterm-bright-cyan', '#bdf4fb'),
      brightWhite: terminalCssValue(styles, '--terminal-xterm-bright-white', '#ffffff'),
    };
  };

  const applyTerminalTheme = (terminal) => {
    const theme = terminalTheme();
    if (terminal.options) {
      terminal.options.theme = theme;
    } else if (typeof terminal.setOption === 'function') {
      terminal.setOption('theme', theme);
    }
    if (typeof terminal.refresh === 'function') {
      terminal.refresh(0, Math.max(0, terminal.rows - 1));
    }
  };

  const applyTerminalThemes = () => {
    terminalControllers.forEach((controller) => {
      applyTerminalTheme(controller.terminal);
    });
  };

  const terminalOptions = () => ({
    allowProposedApi: false,
    convertEol: false,
    cursorBlink: true,
    cursorStyle: 'block',
    fontFamily: 'Consolas, "Cascadia Mono", "DejaVu Sans Mono", monospace',
    fontSize: 14,
    lineHeight: 1,
    letterSpacing: 0,
    scrollback: 5000,
    theme: terminalTheme(),
  });

  const createTerminalController = (session) => {
    const existing = terminalControllers.get(session.id);
    if (existing) {
      existing.session = session;
      return existing;
    }

    const view = document.createElement('div');
    view.className = 'terminal-view';
    view.dataset.terminalView = session.id;

    const surface = document.createElement('div');
    surface.className = 'terminal-surface';
    view.append(surface);
    stack.append(view);

    const terminal = new window.Terminal(terminalOptions());
    const fitAddon = new window.FitAddon.FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(surface);

    const controller = {
      session,
      view,
      terminal,
      fitAddon,
      socket: null,
      resizeTimer: null,
      reconnectTimer: null,
      reconnectDelay: 1000,
      hasConnected: false,
      destroyed: false,
      fitAndSend() {
        try {
          this.fitAddon.fit();
          const surfaceRect = surface.getBoundingClientRect();
          const surfaceStyles = window.getComputedStyle(surface);
          const contentBottom =
            surfaceRect.bottom -
            (Number.parseFloat(surfaceStyles.paddingBottom) || 0);
          for (let attempt = 0; attempt < 3; attempt += 1) {
            const screen = surface.querySelector('.xterm-screen');
            if (
              !screen ||
              screen.getBoundingClientRect().bottom <= contentBottom + 0.5 ||
              this.terminal.rows <= 2
            ) {
              break;
            }
            this.terminal.resize(this.terminal.cols, this.terminal.rows - 1);
          }
        } catch (_) {
          return;
        }
        sendJson(this.socket, {
          type: 'resize',
          cols: this.terminal.cols,
          rows: this.terminal.rows,
        });
      },
      queueResize() {
        window.clearTimeout(this.resizeTimer);
        this.resizeTimer = window.setTimeout(() => this.fitAndSend(), 80);
      },
      clearReconnectTimer() {
        if (this.reconnectTimer) {
          window.clearTimeout(this.reconnectTimer);
          this.reconnectTimer = null;
        }
      },
      scheduleReconnect() {
        if (this.destroyed) {
          return;
        }

        this.clearReconnectTimer();
        const delay = this.reconnectDelay;
        showStatus(`${this.session.title} reconnecting...`, true, false);
        this.reconnectTimer = window.setTimeout(() => {
          this.reconnectTimer = null;
          this.connect();
        }, delay);
        this.reconnectDelay = Math.min(8000, Math.floor(this.reconnectDelay * 1.6));
      },
      connect() {
        if (this.destroyed || socketIsActive(this.socket)) {
          return;
        }

        this.clearReconnectTimer();
        showStatus(`${this.session.title} connecting...`, false, false);
        const nextSocket = new WebSocket(buildSocketUrl(this.session.id).toString());
        this.socket = nextSocket;

        nextSocket.addEventListener('open', () => {
          if (this.socket !== nextSocket) {
            nextSocket.close();
            return;
          }

          this.clearReconnectTimer();
          this.reconnectDelay = 1000;
          if (this.hasConnected) {
            this.terminal.reset();
          }
          this.hasConnected = true;
          showStatus(`${this.session.title} connected.`, false, true);
          this.fitAndSend();
        });

        nextSocket.addEventListener('message', (event) => {
          if (this.socket !== nextSocket) {
            return;
          }

          if (typeof event.data === 'string') {
            this.terminal.write(event.data);
          } else if (event.data instanceof Blob) {
            event.data.text().then((text) => this.terminal.write(text));
          }
        });

        nextSocket.addEventListener('close', () => {
          if (this.socket !== nextSocket) {
            return;
          }

          this.socket = null;
          this.scheduleReconnect();
        });

        nextSocket.addEventListener('error', () => {
          if (this.socket !== nextSocket) {
            return;
          }

          showStatus(`${this.session.title} connection failed.`, true, false);
          try {
            nextSocket.close();
          } catch (_) {
            this.scheduleReconnect();
          }
        });
      },
      reconnect(force) {
        this.clearReconnectTimer();
        this.reconnectDelay = 1000;
        if (force && this.socket && this.socket.readyState !== WebSocket.CLOSED) {
          const staleSocket = this.socket;
          this.socket = null;
          try {
            staleSocket.close();
          } catch (_) {}
        }

        if (socketIsActive(this.socket)) {
          this.fitAndSend();
          return;
        }

        this.connect();
      },
      destroy() {
        this.destroyed = true;
        this.clearReconnectTimer();
        window.clearTimeout(this.resizeTimer);
        if (this.socket) {
          const oldSocket = this.socket;
          this.socket = null;
          try {
            oldSocket.close();
          } catch (_) {}
        }
        this.terminal.dispose();
        this.view.remove();
      },
    };

    terminal.onData((data) => {
      if (!socketIsActive(controller.socket)) {
        controller.reconnect(false);
      }
      sendJson(controller.socket, { type: 'input', data });
    });

    surface.addEventListener('pointerdown', () => terminal.focus(), {
      passive: true,
    });

    terminalControllers.set(session.id, controller);
    controller.connect();
    return controller;
  };

  const renderSessionList = () => {
    sessionList.replaceChildren();
    sessions.forEach((session) => {
      const item = document.createElement('div');
      item.className = 'terminal-session-item';

      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'terminal-session-chip';
      chip.classList.toggle('active', session.id === activeSessionId);
      chip.title = session.title;
      chip.addEventListener('click', () => setActiveSession(session.id));

      const icon = document.createElement('span');
      icon.setAttribute('aria-hidden', 'true');
      icon.textContent = '>_';
      chip.append(icon);

      const title = document.createElement('span');
      title.className = 'terminal-session-title';
      title.textContent = session.title;
      chip.append(title);

      const close = document.createElement('button');
      close.type = 'button';
      close.className = 'terminal-session-close';
      close.disabled = closingSessions.has(session.id);
      close.setAttribute('aria-label', `Close ${session.title}`);
      close.textContent = '\u00d7';
      close.addEventListener('click', (event) => {
        event.stopPropagation();
        closeSession(session.id);
      });

      item.append(chip, close);
      sessionList.append(item);
    });

    newButton.disabled = creatingSession;
    empty.hidden = sessions.length > 0;
  };

  const setActiveSession = (sessionId) => {
    activeSessionId = sessionId;
    terminalControllers.forEach((controller, id) => {
      const active = id === sessionId;
      controller.view.classList.toggle('active', active);
      if (active) {
        controller.queueResize();
        controller.terminal.focus();
      }
    });
    renderSessionList();
  };

  const syncSessions = (nextSessions) => {
    sessions = nextSessions;
    const nextIds = new Set(sessions.map((session) => session.id));
    terminalControllers.forEach((controller, id) => {
      if (!nextIds.has(id)) {
        controller.destroy();
        terminalControllers.delete(id);
      }
    });

    sessions.forEach(createTerminalController);
    if (!activeSessionId || !nextIds.has(activeSessionId)) {
      activeSessionId = sessions[0]?.id || null;
    }

    if (activeSessionId) {
      setActiveSession(activeSessionId);
    } else {
      renderSessionList();
    }
  };

  const loadSessions = async () => {
    showStatus('Loading terminals...', false, false);
    const payload = await apiFetch(sessionsUrl());
    let nextSessions = payload.sessions || [];
    if (nextSessions.length === 0) {
      nextSessions = [await apiFetch(sessionsUrl(), { method: 'POST' })];
    }
    syncSessions(nextSessions);
    showStatus('', false, false);
  };

  const createSession = async () => {
    if (creatingSession) {
      return;
    }

    creatingSession = true;
    renderSessionList();
    showStatus('Creating terminal...', false, false);
    try {
      const created = await apiFetch(sessionsUrl(), { method: 'POST' });
      syncSessions([...sessions, created]);
      setActiveSession(created.id);
      showStatus(`${created.title} ready.`, false, true);
    } catch (error) {
      showStatus(error.message || 'Could not create terminal.', true, false);
    } finally {
      creatingSession = false;
      renderSessionList();
    }
  };

  const closeSession = async (sessionId) => {
    if (closingSessions.has(sessionId)) {
      return;
    }

    closingSessions.add(sessionId);
    renderSessionList();
    try {
      await apiFetch(sessionUrl(sessionId), { method: 'DELETE' });
      const controller = terminalControllers.get(sessionId);
      if (controller) {
        controller.destroy();
        terminalControllers.delete(sessionId);
      }
      const remaining = sessions.filter((session) => session.id !== sessionId);
      activeSessionId =
        activeSessionId === sessionId
          ? remaining[0]?.id || null
          : activeSessionId;
      syncSessions(remaining);
    } catch (error) {
      showStatus(error.message || 'Could not close terminal.', true, false);
    } finally {
      closingSessions.delete(sessionId);
      renderSessionList();
    }
  };

  const reconnectAll = (force) => {
    terminalControllers.forEach((controller) => controller.reconnect(force));
  };

  const resizeAll = () => {
    terminalControllers.forEach((controller) => controller.queueResize());
  };

  const startTerminal = () => {
    if (!sessionList || !newButton || !stack || !empty || !window.Terminal || !window.FitAddon) {
      showStatus('Terminal assets did not load.', true);
      return;
    }

    newButton.addEventListener('click', createSession);
    window.addEventListener('resize', resizeAll);
    window.addEventListener('focus', () => reconnectAll(false));
    window.addEventListener('online', () => reconnectAll(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        reconnectAll(false);
      }
    });
    new MutationObserver(applyTerminalThemes).observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-latitude-theme'],
    });

    loadSessions().catch((error) => {
      showStatus(error.message || 'Could not load terminals.', true, false);
    });
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startTerminal);
  } else {
    startTerminal();
  }
}
