import type { DesktopScreen } from '../../types';
import { nativeDesktopInputRuntime } from './nativeDesktopInputRuntime';
import { nativeDesktopPeerRuntime } from './nativeDesktopPeerRuntime';

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
    <video id="stream" autoplay muted playsinline hidden></video>
    <div id="touch-cursor" hidden></div>
  </div>
  <script>
    window.latitudeViewerStarted = true;
    document.title = ${labelJson};

    const websocketUrl = ${websocketUrlJson};
    const viewOnly = ${viewOnlyJson};
    let configuredScreens = ${screenLayoutJson};
    const viewerBackground = ${viewerBackgroundJson};
    const stage = document.getElementById('stage');
    const canvas = document.getElementById('desktop');
    const context = canvas.getContext('2d', { alpha: false });
    const video = document.getElementById('stream');
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
    let peerConnection = null;
    let controlChannel = null;
    let pointerChannel = null;
    let reconnectTimer = null;
    let reconnectDelay = 1000;
    let reconnectEnabled = true;
    let frameWidth = 0;
    let frameHeight = 0;
    let videoFrameCallback = null;
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
    let controlGranted = false;

    const nativeState = {
      ready: true,
      connected: false,
      status: 'Connecting',
      statusIsError: false,
      viewOnly,
      controlGranted,
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

    const setConnectedStatus = () => {
      if (!viewOnly && !controlGranted) {
        setStatus('Connected · waiting for control');
      } else {
        setStatus('Connected');
      }
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
      if (!screen || !context || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) return;
      if (canvas.width !== screen.width || canvas.height !== screen.height) {
        canvas.width = screen.width;
        canvas.height = screen.height;
      }
      context.drawImage(
        video,
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

    const scheduleVideoFrame = () => {
      if (!peerConnection || videoFrameCallback !== null) return;
      const render = () => {
        videoFrameCallback = null;
        renderFrame();
        scheduleVideoFrame();
      };
      videoFrameCallback = video.requestVideoFrameCallback
        ? video.requestVideoFrameCallback(render)
        : window.requestAnimationFrame(render);
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
      if (!controlChannel || controlChannel.readyState !== 'open') return false;
      controlChannel.send(JSON.stringify(command));
      return true;
    };

    const sendSignal = (message) => {
      if (!socket || socket.readyState !== WebSocket.OPEN) return false;
      socket.send(JSON.stringify(message));
      return true;
    };

    const sendPointer = (buttons) => {
      if (viewOnly || !controlGranted || !frameWidth || !frameHeight) return;
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

    const sendPointerMove = () => {
      if (viewOnly || !controlGranted || !frameWidth || !frameHeight) return;
      const screen = selectedScreen();
      if (!screen) return;
      clampPointer();
      const command = {
        type: 'pointer_move',
        x: (screen.x + pointerX) / frameWidth,
        y: (screen.y + pointerY) / frameHeight,
      };
      if (pointerChannel && pointerChannel.readyState === 'open') {
        if (pointerChannel.bufferedAmount < 4096) {
          pointerChannel.send(JSON.stringify(command));
        }
        updateTouchCursor();
        return;
      }
      send(command);
      updateTouchCursor();
    };

    const clickPointer = () => {
      if (dragLocked) return;
      sendPointer(activeMouseButton);
      window.setTimeout(() => sendPointer(0), 48);
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

    const updateGeometry = (message) => {
      if (Array.isArray(message.screens)) {
        configuredScreens = message.screens;
      }
      const nextWidth = Math.max(1, Number(message.width) || 1);
      const nextHeight = Math.max(1, Number(message.height) || 1);
      const initializePointer = frameWidth === 0 || frameHeight === 0;
      frameWidth = nextWidth;
      frameHeight = nextHeight;
      buildScreensState();
      const screen = selectedScreen();
      if (initializePointer) {
        pointerX = screen ? screen.width / 2 : frameWidth / 2;
        pointerY = screen ? screen.height / 2 : frameHeight / 2;
      }
      renderFrame();
      layoutCanvas();
    };

    const handleControlMessage = (event) => {
      if (typeof event.data !== 'string') return;
      let message;
      try {
        message = JSON.parse(event.data);
      } catch (_) {
        return;
      }
      if (message.type === 'geometry') {
        updateGeometry(message);
      } else if (message.type === 'cursor') {
        canvas.style.cursor = cursorStyles.has(message.cursor) ? message.cursor : 'default';
      } else if (message.type === 'control') {
        controlGranted = message.state === 'granted';
        if (!controlGranted) {
          dragLocked = false;
          pressedModifiers.clear();
        }
        updateNativeState({
          controlGranted,
          dragLocked,
          pressedModifiers: Array.from(pressedModifiers),
        });
        setConnectedStatus();
      } else if (message.type === 'error') {
        setStatus(message.message || 'Desktop stream failed', true);
      }
    };

${nativeDesktopPeerRuntime}

${nativeDesktopInputRuntime}

    window.latitudeMobileCommand = handleNativeCommand;
    window.latitudeReconnect = (force) => {
      clearReconnectTimer();
      reconnectDelay = 1000;
      if (force && socket) {
        const current = socket;
        socket = null;
        current.close();
      }
      if (force) closePeerConnection();
      if (!socket) connect();
    };

    window.addEventListener('resize', layoutCanvas);
    window.addEventListener('focus', () => window.latitudeReconnect(false));
    window.addEventListener('online', () => window.latitudeReconnect(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        window.latitudeReconnect(false);
      } else {
        releaseAllInput();
      }
    });
    window.addEventListener('beforeunload', () => {
      reconnectEnabled = false;
      clearReconnectTimer();
      releaseAllInput();
      socket?.close();
      closePeerConnection();
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
