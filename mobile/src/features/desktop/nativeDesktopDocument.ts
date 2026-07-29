import type { DesktopScreen } from '../../types';

export function nativeDesktopDocument(
  label: string,
  websocketUrl: string,
  viewOnly: boolean,
  screenLayout: DesktopScreen[] = [],
  viewerBackground = '#050505',
): string {
  const labelJson = JSON.stringify(label);
  const websocketUrlJson = JSON.stringify(websocketUrl);
  const viewOnlyJson = JSON.stringify(viewOnly);
  const screenLayoutJson = JSON.stringify(screenLayout);
  const viewerBackgroundCss = viewerBackground.replace(/[;"'<>\\]/g, '');
  const viewerBackgroundJson = JSON.stringify(viewerBackgroundCss);

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <style>
    html,
    body {
      width: 100%;
      height: 100%;
      margin: 0;
      overflow: hidden;
      background: ${viewerBackgroundCss};
      overscroll-behavior: none;
      user-select: none;
      -webkit-touch-callout: none;
      -webkit-user-select: none;
    }

    body {
      position: relative;
      touch-action: none;
    }

    #stage {
      position: absolute;
      inset: 0;
      overflow: hidden;
      background: ${viewerBackgroundCss};
      touch-action: none;
    }

    #desktop {
      position: absolute;
      top: 50%;
      left: 50%;
      display: block;
      background: #050505;
      outline: none;
      touch-action: none;
      transform: translate(-50%, -50%);
    }

    #touch-cursor {
      position: absolute;
      z-index: 2;
      top: 0;
      left: 0;
      width: 22px;
      height: 22px;
      border: 2px solid #2aa79c;
      border-radius: 999px;
      box-shadow: 0 0 0 2px rgba(6, 20, 19, 0.78), 0 8px 20px rgba(0, 0, 0, 0.36);
      opacity: 0.95;
      pointer-events: none;
      transform: translate(-999px, -999px);
    }

    #touch-cursor::after {
      position: absolute;
      top: 50%;
      left: 50%;
      width: 4px;
      height: 4px;
      border-radius: 999px;
      background: #edf4f1;
      content: "";
      transform: translate(-50%, -50%);
    }

    #touch-cursor[hidden] {
      display: none;
    }
  </style>
</head>
<body>
  <div id="stage">
    <canvas id="desktop" tabindex="0"></canvas>
    <div id="touch-cursor" hidden></div>
  </div>
  <script>
    window.latitudeViewerStarted = true;
    document.title = ${labelJson};

    const websocketUrl = ${websocketUrlJson};
    const viewOnly = ${viewOnlyJson};
    const configuredScreens = ${screenLayoutJson};
    const viewerBackground = ${viewerBackgroundJson};
    const stage = document.getElementById('stage');
    const canvas = document.getElementById('desktop');
    const context = canvas.getContext('2d', { alpha: false });
    const touchCursor = document.getElementById('touch-cursor');
    const pointerModeValues = new Set(['touchpad', 'direct']);
    const minZoom = 1;
    const maxZoom = 3;
    const zoomStep = 1.25;
    const touchpadSpeed = 1.32;
    const tapMoveThreshold = 9;
    const wheelStep = 42;
    const cursorStyles = new Set([
      'default',
      'text',
      'pointer',
      'wait',
      'progress',
      'crosshair',
      'help',
      'not-allowed',
      'move',
      'ns-resize',
      'ew-resize',
      'nwse-resize',
      'nesw-resize',
      'n-resize',
      'none',
    ]);

    let socket = null;
    let reconnectTimer = null;
    let reconnectDelay = 1000;
    let reconnectEnabled = true;
    let frameWidth = 0;
    let frameHeight = 0;
    let latestImage = null;
    let frameGeneration = 0;
    let autoScale = true;
    let zoomLevel = 1;
    let selectedScreenId =
      configuredScreens.length > 1
        ? ((configuredScreens.find((screen) => screen.primary) || configuredScreens[0]).id)
        : 'all';
    let pointerMode = 'touchpad';
    let activeMouseButton = 0x1;
    let dragLocked = false;
    let pointerX = 0;
    let pointerY = 0;
    let touchState = null;
    let pressedModifiers = new Set();
    let nativeStateTimer = null;

    const nativeState = {
      ready: true,
      connected: false,
      status: 'Connecting',
      statusIsError: false,
      viewOnly,
      autoScale,
      zoomLevel,
      selectedScreenId,
      screens: configuredScreens,
      pointerMode,
      activeMouseButton,
      dragLocked,
      pressedModifiers: [],
      credentialsRequired: null,
    };

    const postNativeMessage = (payload) => {
      if (!window.ReactNativeWebView || typeof window.ReactNativeWebView.postMessage !== 'function') {
        return;
      }
      try {
        window.ReactNativeWebView.postMessage(JSON.stringify(payload));
      } catch (_) {}
    };

    const flushNativeState = () => {
      nativeStateTimer = null;
      postNativeMessage({ type: 'desktop-state', state: nativeState });
    };

    const updateNativeState = (patch) => {
      Object.assign(nativeState, patch);
      if (!nativeStateTimer) {
        nativeStateTimer = window.setTimeout(flushNativeState, 0);
      }
    };

    const setStatus = (status, statusIsError = false) => {
      updateNativeState({ status, statusIsError: Boolean(statusIsError) });
    };

    const normalizedScreens = () =>
      configuredScreens
        .map((screen, index) => ({
          id: String(screen.id || 'screen-' + (index + 1)),
          label: String(screen.label || index + 1),
          title: String(screen.title || 'Screen ' + (index + 1)),
          x: Math.max(0, Number(screen.x) || 0),
          y: Math.max(0, Number(screen.y) || 0),
          width: Math.max(1, Number(screen.width) || 1),
          height: Math.max(1, Number(screen.height) || 1),
          primary: Boolean(screen.primary),
        }))
        .filter(
          (screen) =>
            frameWidth > 0 &&
            frameHeight > 0 &&
            screen.x + screen.width <= frameWidth &&
            screen.y + screen.height <= frameHeight,
        );

    const selectedScreen = () => {
      const screens = normalizedScreens();
      if (screens.length < 2 || selectedScreenId === 'all') {
        return { id: 'all', x: 0, y: 0, width: frameWidth, height: frameHeight };
      }
      return screens.find((screen) => screen.id === selectedScreenId) || screens[0];
    };

    const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

    const clampPointer = () => {
      const screen = selectedScreen();
      if (!screen) return;
      pointerX = clamp(pointerX, 0, Math.max(0, screen.width - 1));
      pointerY = clamp(pointerY, 0, Math.max(0, screen.height - 1));
    };

    const layoutCanvas = () => {
      const screen = selectedScreen();
      if (!screen || !screen.width || !screen.height) return;
      const fitScale = Math.min(stage.clientWidth / screen.width, stage.clientHeight / screen.height);
      const scale = (autoScale ? fitScale : 1) * zoomLevel;
      canvas.style.width = Math.max(1, screen.width * scale) + 'px';
      canvas.style.height = Math.max(1, screen.height * scale) + 'px';
      updateTouchCursor();
    };

    const renderFrame = () => {
      const screen = selectedScreen();
      if (!latestImage || !screen || !context) return;
      if (canvas.width !== screen.width || canvas.height !== screen.height) {
        canvas.width = screen.width;
        canvas.height = screen.height;
      }
      context.drawImage(
        latestImage,
        screen.x,
        screen.y,
        screen.width,
        screen.height,
        0,
        0,
        screen.width,
        screen.height,
      );
      clampPointer();
      layoutCanvas();
    };

    const updateTouchCursor = () => {
      const screen = selectedScreen();
      if (
        viewOnly ||
        pointerMode !== 'touchpad' ||
        !screen ||
        !screen.width ||
        !screen.height
      ) {
        touchCursor.hidden = true;
        return;
      }
      const bounds = canvas.getBoundingClientRect();
      const stageBounds = stage.getBoundingClientRect();
      touchCursor.hidden = false;
      touchCursor.style.transform =
        'translate(' +
        Math.round(bounds.left - stageBounds.left + (pointerX / screen.width) * bounds.width - 11) +
        'px,' +
        Math.round(bounds.top - stageBounds.top + (pointerY / screen.height) * bounds.height - 11) +
        'px)';
    };

    const send = (command) => {
      if (!socket || socket.readyState !== WebSocket.OPEN) return false;
      socket.send(JSON.stringify(command));
      return true;
    };

    const sendPointer = (buttons) => {
      if (viewOnly || !frameWidth || !frameHeight) return;
      const screen = selectedScreen();
      if (!screen) return;
      clampPointer();
      send({
        type: 'pointer',
        x: (screen.x + pointerX) / frameWidth,
        y: (screen.y + pointerY) / frameHeight,
        buttons,
      });
      updateTouchCursor();
    };

    const clickPointer = () => {
      if (dragLocked) return;
      sendPointer(activeMouseButton);
      window.setTimeout(() => sendPointer(0), 48);
    };

    const decodeFrame = (data) => {
      const generation = ++frameGeneration;
      const blob = new Blob([data], { type: 'image/jpeg' });
      const objectUrl = URL.createObjectURL(blob);
      const image = new Image();
      image.onload = () => {
        URL.revokeObjectURL(objectUrl);
        if (generation !== frameGeneration) return;
        latestImage = image;
        renderFrame();
      };
      image.onerror = () => {
        URL.revokeObjectURL(objectUrl);
        setStatus('Desktop frame could not be decoded', true);
      };
      image.src = objectUrl;
    };

    const buildScreensState = () => {
      const screens = normalizedScreens();
      if (screens.length < 2) {
        selectedScreenId = 'all';
      } else if (!screens.some((screen) => screen.id === selectedScreenId)) {
        selectedScreenId = (screens.find((screen) => screen.primary) || screens[0]).id;
      }
      updateNativeState({ screens, selectedScreenId });
    };

    const clearReconnectTimer = () => {
      if (reconnectTimer) {
        window.clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const scheduleReconnect = () => {
      if (!reconnectEnabled || reconnectTimer) return;
      const delay = reconnectDelay;
      setStatus('Reconnecting', true);
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null;
        connect();
      }, delay);
      reconnectDelay = Math.min(8000, Math.floor(reconnectDelay * 1.6));
    };

    const connect = () => {
      if (socket) return;
      clearReconnectTimer();
      setStatus('Connecting');
      const nextSocket = new WebSocket(websocketUrl);
      nextSocket.binaryType = 'arraybuffer';
      socket = nextSocket;
      nextSocket.onopen = () => {
        if (socket !== nextSocket) return;
        reconnectDelay = 1000;
        updateNativeState({ connected: true });
        setStatus('Connected');
      };
      nextSocket.onmessage = (event) => {
        if (socket !== nextSocket) return;
        if (typeof event.data !== 'string') {
          decodeFrame(event.data);
          return;
        }
        let message;
        try {
          message = JSON.parse(event.data);
        } catch (_) {
          return;
        }
        if (message.type === 'hello') {
          frameWidth = Math.max(1, Number(message.width) || 1);
          frameHeight = Math.max(1, Number(message.height) || 1);
          buildScreensState();
          const screen = selectedScreen();
          pointerX = screen ? screen.width / 2 : frameWidth / 2;
          pointerY = screen ? screen.height / 2 : frameHeight / 2;
          renderFrame();
          updateNativeState({ connected: true });
          setStatus('Connected');
        } else if (message.type === 'cursor') {
          canvas.style.cursor = cursorStyles.has(message.cursor) ? message.cursor : 'default';
        } else if (message.type === 'error') {
          setStatus(message.message || 'Desktop connection failed', true);
        }
      };
      nextSocket.onerror = () => {
        if (socket === nextSocket) setStatus('Desktop connection failed', true);
      };
      nextSocket.onclose = () => {
        if (socket !== nextSocket) return;
        socket = null;
        pressedModifiers.clear();
        updateModifiers();
        updateNativeState({ connected: false });
        scheduleReconnect();
      };
    };

    const keyDefinitions = {
      backspace: { vk: 8 },
      tab: { vk: 9 },
      enter: { vk: 13 },
      escape: { vk: 27 },
      delete: { vk: 46, extended: true },
      home: { vk: 36, extended: true },
      left: { vk: 37, extended: true },
      up: { vk: 38, extended: true },
      right: { vk: 39, extended: true },
      down: { vk: 40, extended: true },
      pageup: { vk: 33, extended: true },
      pagedown: { vk: 34, extended: true },
      end: { vk: 35, extended: true },
    };

    const modifierDefinitions = {
      shift: { vk: 16 },
      control: { vk: 17 },
      alt: { vk: 18 },
      meta: { vk: 91, extended: true },
    };

    const sendKey = (definition, down) => {
      if (viewOnly || !definition) return;
      send({
        type: 'key',
        vk: definition.vk,
        down: Boolean(down),
        extended: Boolean(definition.extended),
      });
    };

    const pressKey = (definition) => {
      sendKey(definition, true);
      sendKey(definition, false);
    };

    const updateModifiers = () => {
      updateNativeState({ pressedModifiers: Array.from(pressedModifiers) });
    };

    const releaseModifiers = () => {
      for (const modifier of Array.from(pressedModifiers)) {
        sendKey(modifierDefinitions[modifier], false);
      }
      pressedModifiers.clear();
      updateModifiers();
    };

    const toggleModifier = (modifier) => {
      const definition = modifierDefinitions[modifier];
      if (!definition || viewOnly) return;
      if (pressedModifiers.has(modifier)) {
        sendKey(definition, false);
        pressedModifiers.delete(modifier);
      } else {
        sendKey(definition, true);
        pressedModifiers.add(modifier);
      }
      updateModifiers();
    };

    const sendShortcut = (shortcut) => {
      const shortcutMap = {
        'ctrl-a': { modifiers: ['control'], vk: 65 },
        'ctrl-c': { modifiers: ['control'], vk: 67 },
        'ctrl-v': { modifiers: ['control'], vk: 86 },
        'ctrl-x': { modifiers: ['control'], vk: 88 },
        'ctrl-z': { modifiers: ['control'], vk: 90 },
        'ctrl-alt-del': { modifiers: ['control', 'alt'], vk: 46, extended: true },
      };
      const definition = shortcutMap[shortcut];
      if (!definition || viewOnly) return;
      releaseModifiers();
      for (const modifier of definition.modifiers) {
        sendKey(modifierDefinitions[modifier], true);
      }
      pressKey({ vk: definition.vk, extended: definition.extended });
      for (const modifier of definition.modifiers.slice().reverse()) {
        sendKey(modifierDefinitions[modifier], false);
      }
    };

    const touchPoint = (touch) => {
      const bounds = canvas.getBoundingClientRect();
      const screen = selectedScreen();
      if (!screen || bounds.width <= 0 || bounds.height <= 0) return null;
      return {
        x: clamp(((touch.clientX - bounds.left) / bounds.width) * screen.width, 0, screen.width - 1),
        y: clamp(((touch.clientY - bounds.top) / bounds.height) * screen.height, 0, screen.height - 1),
      };
    };

    const handleTouchStart = (event) => {
      if (viewOnly) return;
      event.preventDefault();
      const touch = event.touches[0];
      if (!touch) return;
      const point = touchPoint(touch);
      if (!point) return;
      if (pointerMode === 'direct') {
        pointerX = point.x;
        pointerY = point.y;
        sendPointer(dragLocked ? activeMouseButton : 0);
      }
      touchState = {
        count: event.touches.length,
        startX: touch.clientX,
        startY: touch.clientY,
        lastX: touch.clientX,
        lastY: touch.clientY,
        moved: false,
        wheelX: 0,
        wheelY: 0,
      };
    };

    const handleTouchMove = (event) => {
      if (viewOnly || !touchState) return;
      event.preventDefault();
      const touch = event.touches[0];
      if (!touch) return;
      const dx = touch.clientX - touchState.lastX;
      const dy = touch.clientY - touchState.lastY;
      touchState.lastX = touch.clientX;
      touchState.lastY = touch.clientY;
      if (
        Math.abs(touch.clientX - touchState.startX) > tapMoveThreshold ||
        Math.abs(touch.clientY - touchState.startY) > tapMoveThreshold
      ) {
        touchState.moved = true;
      }

      if (event.touches.length >= 2 || touchState.count >= 2) {
        touchState.wheelX += dx;
        touchState.wheelY += dy;
        if (Math.abs(touchState.wheelY) >= wheelStep || Math.abs(touchState.wheelX) >= wheelStep) {
          send({
            type: 'wheel',
            delta_x: touchState.wheelX > wheelStep ? 120 : touchState.wheelX < -wheelStep ? -120 : 0,
            delta_y: touchState.wheelY > wheelStep ? -120 : touchState.wheelY < -wheelStep ? 120 : 0,
          });
          touchState.wheelX = 0;
          touchState.wheelY = 0;
        }
        return;
      }

      if (pointerMode === 'direct') {
        const point = touchPoint(touch);
        if (point) {
          pointerX = point.x;
          pointerY = point.y;
        }
      } else {
        const screen = selectedScreen();
        const bounds = canvas.getBoundingClientRect();
        if (screen && bounds.width > 0 && bounds.height > 0) {
          pointerX += (dx / bounds.width) * screen.width * touchpadSpeed;
          pointerY += (dy / bounds.height) * screen.height * touchpadSpeed;
        }
      }
      sendPointer(dragLocked ? activeMouseButton : 0);
    };

    const handleTouchEnd = (event) => {
      if (viewOnly || !touchState) return;
      event.preventDefault();
      const wasTap = !touchState.moved && touchState.count === 1;
      touchState = null;
      if (wasTap) clickPointer();
      else if (!dragLocked) sendPointer(0);
    };

    for (const type of ['touchstart', 'touchmove', 'touchend', 'touchcancel']) {
      canvas.addEventListener(
        type,
        type === 'touchstart'
          ? handleTouchStart
          : type === 'touchmove'
            ? handleTouchMove
            : handleTouchEnd,
        { passive: false },
      );
    }

    const handleNativeCommand = (command) => {
      if (!command || typeof command !== 'object') return;
      const type = command.type || command.action;
      if (type === 'requestState') {
        flushNativeState();
      } else if (type === 'toggleScale') {
        if (zoomLevel > 1 || !autoScale) {
          zoomLevel = 1;
          autoScale = true;
        } else {
          autoScale = false;
        }
        updateNativeState({ autoScale, zoomLevel });
        layoutCanvas();
      } else if (type === 'zoomIn' || type === 'zoomOut') {
        const factor = type === 'zoomIn' ? zoomStep : 1 / zoomStep;
        zoomLevel = clamp(zoomLevel * factor, minZoom, maxZoom);
        updateNativeState({ zoomLevel });
        layoutCanvas();
      } else if (type === 'selectScreen') {
        selectedScreenId = String(command.screenId || 'all');
        const screen = selectedScreen();
        if (screen) {
          pointerX = screen.width / 2;
          pointerY = screen.height / 2;
        }
        updateNativeState({ selectedScreenId });
        renderFrame();
      } else if (type === 'setPointerMode' && pointerModeValues.has(command.mode)) {
        pointerMode = command.mode;
        updateNativeState({ pointerMode });
        updateTouchCursor();
      } else if (type === 'setMouseButton') {
        const mask = Number(command.buttonMask);
        activeMouseButton = [0x1, 0x2, 0x4].includes(mask) ? mask : 0x1;
        if (dragLocked) sendPointer(activeMouseButton);
        updateNativeState({ activeMouseButton });
      } else if (type === 'toggleDragLock') {
        dragLocked = !dragLocked;
        sendPointer(dragLocked ? activeMouseButton : 0);
        updateNativeState({ dragLocked });
      } else if (type === 'pressKey') {
        pressKey(keyDefinitions[command.key]);
      } else if (type === 'toggleModifier') {
        toggleModifier(command.modifier);
      } else if (type === 'releaseModifiers') {
        releaseModifiers();
      } else if (type === 'shortcut') {
        sendShortcut(command.shortcut);
      } else if (type === 'sendText') {
        send({ type: 'text', text: String(command.text || '') });
      } else if (type === 'refresh') {
        send({ type: 'refresh' });
      } else if (type === 'reconnect') {
        window.latitudeReconnect(Boolean(command.force));
      }
    };

    window.latitudeMobileCommand = handleNativeCommand;
    window.latitudeReconnect = (force) => {
      clearReconnectTimer();
      reconnectDelay = 1000;
      if (force && socket) {
        const current = socket;
        socket = null;
        current.close();
      }
      if (!socket) connect();
    };

    window.addEventListener('resize', layoutCanvas);
    window.addEventListener('focus', () => window.latitudeReconnect(false));
    window.addEventListener('online', () => window.latitudeReconnect(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        window.latitudeReconnect(false);
      } else {
        releaseModifiers();
      }
    });
    window.addEventListener('beforeunload', () => {
      reconnectEnabled = false;
      clearReconnectTimer();
      releaseModifiers();
      if (dragLocked) sendPointer(0);
      socket?.close();
    });
    window.addEventListener('error', (event) => {
      setStatus(event.message || 'Viewer error', true);
    });
    window.addEventListener('unhandledrejection', (event) => {
      const reason = event.reason;
      setStatus((reason && reason.message) || reason || 'Viewer error', true);
    });

    document.documentElement.style.background = viewerBackground;
    document.body.style.background = viewerBackground;
    stage.style.background = viewerBackground;
    flushNativeState();
    connect();
  </script>
</body>
</html>`;
}
