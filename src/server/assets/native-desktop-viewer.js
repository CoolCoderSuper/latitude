import {
  isExtendedKey,
  pointerButtonMask,
  virtualKeyFor,
} from './native-desktop-input.js?v=2';
import { NativeDesktopPeer } from './native-desktop-peer.js?v=4';

const workspace = document.querySelector('[data-desktop-workspace]');

if (workspace) {
  const target = workspace.querySelector('[data-desktop-target]');
  const status = workspace.querySelector('[data-desktop-status]');
  const screenSwitcher = workspace.querySelector('[data-desktop-screens]');
  const resolutionSelect = workspace.querySelector('[data-desktop-resolution]');
  const scaleButton = workspace.querySelector('[data-desktop-scale]');
  const fullscreenButton = workspace.querySelector('[data-desktop-fullscreen]');
  const viewOnly = workspace.dataset.viewOnly !== 'false';
  const actionPath = workspace.dataset.actionPath || '/_desktop';
  let configuredScreens = parseArray(workspace.dataset.screenLayout);
  let resolutionOptions = parseArray(workspace.dataset.resolutionOptions);
  const canvas = document.createElement('canvas');
  const context = canvas.getContext('2d', { alpha: false });
  const video = document.createElement('video');
  let socket = null;
  let peerSession = null;
  let reconnectTimer = null;
  let reconnectDelay = 1000;
  let reconnectEnabled = true;
  let frameWidth = 0;
  let frameHeight = 0;
  let videoFrameCallback = null;
  let pointerMoveFrame = null;
  let pendingPointerMove = null;
  let selectedScreenId = 'all';
  let autoScale = true;
  let pointerButtons = 0;
  const pressedKeys = new Map();
  let resolutionChanging = false;
  let controlState = viewOnly ? 'disabled' : 'pending';
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

  canvas.className = 'native-desktop-canvas';
  canvas.tabIndex = 0;
  canvas.setAttribute('aria-label', 'Remote desktop');
  video.autoplay = true;
  video.muted = true;
  video.playsInline = true;
  video.hidden = true;
  target.classList.add('native-desktop-target');
  target.replaceChildren(canvas, video);

  function parseArray(value) {
    try {
      const parsed = JSON.parse(value || '[]');
      return Array.isArray(parsed) ? parsed : [];
    } catch (_) {
      return [];
    }
  }

  function setStatus(message, isError = false) {
    status.textContent = message;
    status.classList.toggle('error', Boolean(isError));
  }

  function hasControl() {
    return !viewOnly && controlState === 'granted';
  }

  function setConnectedStatus() {
    if (!viewOnly && controlState === 'waiting') {
      setStatus('Connected · waiting for control');
    } else if (!viewOnly && controlState === 'pending') {
      setStatus('Connected · checking control');
    } else {
      setStatus('Connected');
    }
  }

  function buildSocketUrl() {
    const fallback = `${window.location.pathname.replace(/\/$/, '')}/ws`;
    const url = new URL(workspace.dataset.wsPath || fallback, window.location.href);
    url.protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    if (workspace.dataset.wsToken) {
      url.searchParams.set('token', workspace.dataset.wsToken);
    }
    return url;
  }

  function screenOptions() {
    if (!frameWidth || !frameHeight) {
      return [];
    }
    const all = {
      id: 'all',
      label: 'All',
      title: 'All screens',
      x: 0,
      y: 0,
      width: frameWidth,
      height: frameHeight,
    };
    const screens = configuredScreens
      .map((screen, index) => ({
        id: String(screen.id || `screen-${index + 1}`),
        label: String(screen.label || index + 1),
        title: String(screen.title || `Screen ${index + 1}`),
        x: Math.max(0, Number(screen.x) || 0),
        y: Math.max(0, Number(screen.y) || 0),
        width: Math.max(1, Number(screen.width) || 1),
        height: Math.max(1, Number(screen.height) || 1),
        primary: Boolean(screen.primary),
      }))
      .filter(
        (screen) =>
          screen.x < frameWidth &&
          screen.y < frameHeight &&
          screen.x + screen.width <= frameWidth &&
          screen.y + screen.height <= frameHeight,
      );
    return screens.length > 1 ? [all, ...screens] : [all];
  }

  function selectedScreen() {
    const options = screenOptions();
    return options.find((screen) => screen.id === selectedScreenId) || options[0] || null;
  }

  function renderScreenSwitcher() {
    const options = screenOptions();
    screenSwitcher.replaceChildren();
    screenSwitcher.hidden = options.length <= 1;
    for (const screen of options) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'desktop-control-button';
      button.textContent = screen.label;
      button.title = screen.title;
      button.classList.toggle('active', screen.id === selectedScreenId);
      button.setAttribute('aria-pressed', String(screen.id === selectedScreenId));
      button.addEventListener('click', () => {
        selectedScreenId = screen.id;
        renderScreenSwitcher();
        renderFrame();
      });
      screenSwitcher.appendChild(button);
    }
  }

  function renderResolutionOptions() {
    resolutionSelect.replaceChildren();
    resolutionSelect.hidden = resolutionOptions.length === 0;
    for (const resolution of resolutionOptions) {
      const option = document.createElement('option');
      option.value = `${resolution.width}x${resolution.height}`;
      option.textContent = `${resolution.width} × ${resolution.height}`;
      option.selected = Boolean(resolution.current);
      resolutionSelect.appendChild(option);
    }
  }

  function updateScaleButton() {
    scaleButton.textContent = autoScale ? 'Fit' : '1:1';
    scaleButton.classList.toggle('active', autoScale);
    scaleButton.setAttribute('aria-pressed', String(autoScale));
  }

  function updateFullscreenButton() {
    const active = document.fullscreenElement === workspace;
    fullscreenButton.textContent = active ? 'Exit' : 'Full';
    fullscreenButton.classList.toggle('active', active);
    fullscreenButton.setAttribute('aria-pressed', String(active));
  }

  function renderFrame() {
    const screen = selectedScreen();
    if (!context || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !screen) {
      return;
    }
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
    canvas.classList.toggle('fit', autoScale);
    target.classList.toggle('native-desktop-native-size', !autoScale);
  }

  function scheduleVideoFrame() {
    if (!peerSession || videoFrameCallback !== null) {
      return;
    }
    const render = () => {
      videoFrameCallback = null;
      renderFrame();
      scheduleVideoFrame();
    };
    videoFrameCallback = video.requestVideoFrameCallback
      ? video.requestVideoFrameCallback(render)
      : window.requestAnimationFrame(render);
  }

  function send(command) {
    return peerSession?.sendControl(command) || false;
  }

  function sendSignal(message) {
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return false;
    }
    socket.send(JSON.stringify(message));
    return true;
  }

  function pointerPosition(event) {
    const screen = selectedScreen();
    const bounds = canvas.getBoundingClientRect();
    if (!screen || bounds.width <= 0 || bounds.height <= 0 || !frameWidth || !frameHeight) {
      return null;
    }
    const localX = Math.min(1, Math.max(0, (event.clientX - bounds.left) / bounds.width));
    const localY = Math.min(1, Math.max(0, (event.clientY - bounds.top) / bounds.height));
    return {
      x: (screen.x + localX * screen.width) / frameWidth,
      y: (screen.y + localY * screen.height) / frameHeight,
    };
  }

  function sendPointer(event, buttons = pointerButtons) {
    if (!hasControl()) {
      return;
    }
    cancelPendingPointerMove();
    const point = pointerPosition(event);
    if (point) {
      send({ type: 'pointer', x: point.x, y: point.y, buttons });
    }
  }

  function sendPointerMove(event) {
    if (!hasControl()) {
      return;
    }
    const point = pointerPosition(event);
    if (!point) {
      return;
    }
    pendingPointerMove = { type: 'pointer_move', x: point.x, y: point.y };
    if (pointerMoveFrame === null) {
      pointerMoveFrame = window.requestAnimationFrame(flushPointerMove);
    }
  }

  function flushPointerMove() {
    pointerMoveFrame = null;
    if (!pendingPointerMove || !hasControl()) {
      pendingPointerMove = null;
      return;
    }
    if (peerSession?.sendPointer(pendingPointerMove)) {
      pendingPointerMove = null;
    }
    if (pendingPointerMove !== null) {
      pointerMoveFrame = window.requestAnimationFrame(flushPointerMove);
    }
  }

  function cancelPendingPointerMove() {
    if (pointerMoveFrame !== null) {
      window.cancelAnimationFrame(pointerMoveFrame);
      pointerMoveFrame = null;
    }
    pendingPointerMove = null;
  }

  function clearReconnectTimer() {
    if (reconnectTimer !== null) {
      window.clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  }

  function scheduleReconnect() {
    if (!reconnectEnabled || reconnectTimer !== null) {
      return;
    }
    setStatus('Reconnecting', true);
    const delay = reconnectDelay;
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
    reconnectDelay = Math.min(8000, Math.floor(reconnectDelay * 1.6));
  }

  function updateGeometry(message) {
    let screensChanged = false;
    if (Array.isArray(message.screens)) {
      configuredScreens = message.screens;
      workspace.dataset.screenLayout = JSON.stringify(configuredScreens);
      screensChanged = true;
    }
    const nextWidth = Math.max(1, Number(message.width) || 1);
    const nextHeight = Math.max(1, Number(message.height) || 1);
    if (frameWidth === nextWidth && frameHeight === nextHeight && !screensChanged) {
      return;
    }
    frameWidth = nextWidth;
    frameHeight = nextHeight;
    if (!screenOptions().some((screen) => screen.id === selectedScreenId)) {
      selectedScreenId = 'all';
    }
    renderScreenSwitcher();
    renderFrame();
  }

  function handleControlMessage(event) {
    if (typeof event.data !== 'string') {
      return;
    }
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
      controlState = message.state === 'granted' ? 'granted' : 'waiting';
      if (hasControl()) {
        canvas.focus({ preventScroll: true });
      } else {
        pointerButtons = 0;
        pressedKeys.clear();
      }
      setConnectedStatus();
    } else if (message.type === 'error') {
      setStatus(message.message || 'Desktop stream failed', true);
    }
  }

  function closePeerConnection() {
    const currentPeer = peerSession;
    peerSession = null;
    cancelPendingPointerMove();
    if (videoFrameCallback !== null) {
      if (typeof video.cancelVideoFrameCallback === 'function') {
        video.cancelVideoFrameCallback(videoFrameCallback);
      } else {
        window.cancelAnimationFrame(videoFrameCallback);
      }
    }
    videoFrameCallback = null;
    video.srcObject = null;
    currentPeer?.close();
  }

  async function startPeerConnection(iceServers, h264ProfileLevelId) {
    if (peerSession) {
      return;
    }
    const peer = new NativeDesktopPeer({
      onControlOpen: () => {
        setConnectedStatus();
      },
      onControlMessage: handleControlMessage,
      onControlError: () => {
        setStatus('Desktop control channel failed', true);
      },
      onTrack: (event) => {
        video.srcObject = event.streams[0] || new MediaStream([event.track]);
        void video.play().then(scheduleVideoFrame).catch((error) => {
          setStatus(error?.message || 'Desktop video could not start', true);
        });
      },
      onConnectionState: (state) => {
        if (state === 'connected') {
          reconnectDelay = 1000;
          setConnectedStatus();
        } else if (state === 'connecting') {
          setStatus('Connecting media');
        } else if (state === 'failed') {
          setStatus('WebRTC connection failed', true);
          socket?.close();
        }
      },
      onIceCandidate: (candidate) => {
        sendSignal({ type: 'candidate', candidate });
      },
    });
    peerSession = peer;
    const offer = await peer.start(iceServers, h264ProfileLevelId);
    if (peerSession !== peer || !offer) {
      return;
    }
    setStatus('Negotiating');
    if (sendSignal({ type: 'offer', sdp: offer })) {
      peer.releaseIceCandidates();
    }
  }

  function connect() {
    if (socket) {
      return;
    }
    clearReconnectTimer();
    setStatus('Connecting');
    const nextSocket = new WebSocket(buildSocketUrl());
    socket = nextSocket;

    nextSocket.addEventListener('open', () => {
      if (socket !== nextSocket) return;
      reconnectDelay = 1000;
      setStatus('Negotiating');
    });
    nextSocket.addEventListener('message', async (event) => {
      if (socket !== nextSocket) return;
      if (typeof event.data !== 'string') return;
      let message;
      try {
        message = JSON.parse(event.data);
      } catch (_) {
        return;
      }
      if (message.type === 'hello') {
        updateGeometry(message);
        try {
          await startPeerConnection(message.ice_servers, message.h264_profile_level_id);
        } catch (error) {
          setStatus(error?.message || 'WebRTC could not be started', true);
          nextSocket.close();
        }
      } else if (message.type === 'answer') {
        if (!peerSession) return;
        try {
          await peerSession.acceptAnswer(message.sdp);
          setStatus('Connecting media');
        } catch (error) {
          setStatus(error?.message || 'WebRTC answer was rejected', true);
          nextSocket.close();
        }
      } else if (message.type === 'candidate') {
        if (!peerSession || !message.candidate) return;
        try {
          await peerSession.addCandidate(message.candidate);
        } catch (error) {
          setStatus(error?.message || 'WebRTC ICE candidate was rejected', true);
          nextSocket.close();
        }
      } else if (message.type === 'error') {
        setStatus(message.message || 'Desktop connection failed', true);
      }
    });
    nextSocket.addEventListener('close', () => {
      if (socket !== nextSocket) return;
      socket = null;
      pointerButtons = 0;
      pressedKeys.clear();
      controlState = viewOnly ? 'disabled' : 'pending';
      closePeerConnection();
      scheduleReconnect();
    });
    nextSocket.addEventListener('error', () => {
      if (socket === nextSocket) setStatus('Desktop connection failed', true);
    });
  }

  canvas.addEventListener('pointerdown', (event) => {
    if (!hasControl()) return;
    event.preventDefault();
    canvas.focus({ preventScroll: true });
    canvas.setPointerCapture?.(event.pointerId);
    pointerButtons |= pointerButtonMask(event.button);
    sendPointer(event);
  });
  canvas.addEventListener('pointermove', (event) => {
    if (!hasControl()) return;
    event.preventDefault();
    sendPointerMove(event);
  });
  const releasePointer = (event) => {
    if (!hasControl()) return;
    event.preventDefault();
    pointerButtons &= ~pointerButtonMask(event.button);
    sendPointer(event);
  };
  canvas.addEventListener('pointerup', releasePointer);
  canvas.addEventListener('pointercancel', (event) => {
    if (!hasControl()) return;
    event.preventDefault();
    releaseAllInput();
  });
  canvas.addEventListener('contextmenu', (event) => event.preventDefault());
  canvas.addEventListener(
    'wheel',
    (event) => {
      if (!hasControl()) return;
      event.preventDefault();
      sendPointer(event);
      send({
        type: 'wheel',
        delta_x: event.deltaX === 0 ? 0 : event.deltaX > 0 ? 120 : -120,
        delta_y: event.deltaY === 0 ? 0 : event.deltaY > 0 ? -120 : 120,
      });
    },
    { passive: false },
  );
  canvas.addEventListener('keydown', (event) => {
    if (!hasControl()) return;
    const vk = virtualKeyFor(event);
    if (!vk) return;
    event.preventDefault();
    const extended = isExtendedKey(event.code || '');
    pressedKeys.set(event.code || `${vk}:${extended}`, { vk, extended });
    send({ type: 'key', vk, down: true, extended });
  });
  canvas.addEventListener('keyup', (event) => {
    if (!hasControl()) return;
    const vk = virtualKeyFor(event);
    if (!vk) return;
    event.preventDefault();
    const extended = isExtendedKey(event.code || '');
    pressedKeys.delete(event.code || `${vk}:${extended}`);
    send({ type: 'key', vk, down: false, extended });
  });
  const releaseAllInput = () => {
    const shouldRelease = hasControl();
    cancelPendingPointerMove();
    pointerButtons = 0;
    pressedKeys.clear();
    if (shouldRelease) {
      send({ type: 'release_input' });
    }
  };
  canvas.addEventListener('blur', releaseAllInput);
  canvas.addEventListener('paste', (event) => {
    if (!hasControl()) return;
    const text = event.clipboardData?.getData('text/plain');
    if (text) {
      event.preventDefault();
      send({ type: 'text', text });
    }
  });

  scaleButton.addEventListener('click', () => {
    autoScale = !autoScale;
    updateScaleButton();
    renderFrame();
  });
  fullscreenButton.addEventListener('click', async () => {
    if (document.fullscreenElement === workspace) {
      await document.exitFullscreen?.();
    } else {
      await workspace.requestFullscreen?.();
    }
    updateFullscreenButton();
  });
  document.addEventListener('fullscreenchange', updateFullscreenButton);
  resolutionSelect.addEventListener('change', async () => {
    if (resolutionChanging) return;
    const [width, height] = resolutionSelect.value.split('x').map(Number);
    if (!width || !height) return;
    resolutionChanging = true;
    resolutionSelect.disabled = true;
    setStatus('Changing resolution');
    try {
      const headers = { 'content-type': 'application/json' };
      if (workspace.dataset.wsToken) {
        headers.authorization = `Bearer ${workspace.dataset.wsToken}`;
      }
      const response = await fetch(actionPath, {
        method: 'PATCH',
        credentials: 'same-origin',
        headers,
        body: JSON.stringify({
          action: 'set_resolution',
          screen_id: selectedScreenId.startsWith('display-') ? selectedScreenId : null,
          width,
          height,
        }),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(payload.error || `Resolution change failed (${response.status})`);
      }
      if (Array.isArray(payload.resolutions)) {
        resolutionOptions = payload.resolutions;
        workspace.dataset.resolutionOptions = JSON.stringify(resolutionOptions);
      }
      if (!screenOptions().some((screen) => screen.id === selectedScreenId)) {
        selectedScreenId = 'all';
      }
      renderScreenSwitcher();
      renderResolutionOptions();
      setStatus('Resolution changed');
    } catch (error) {
      setStatus(error?.message || 'Resolution change failed', true);
    } finally {
      resolutionChanging = false;
      resolutionSelect.disabled = false;
    }
  });
  window.addEventListener('focus', () => {
    if (!socket) connect();
  });
  window.addEventListener('blur', releaseAllInput);
  window.addEventListener('online', () => {
    if (socket) {
      const current = socket;
      socket = null;
      current.close();
    }
    closePeerConnection();
    connect();
  });
  window.addEventListener('beforeunload', () => {
    reconnectEnabled = false;
    clearReconnectTimer();
    releaseAllInput();
    socket?.close();
    closePeerConnection();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      releaseAllInput();
    }
  });

  renderResolutionOptions();
  renderScreenSwitcher();
  updateScaleButton();
  updateFullscreenButton();
  connect();
}
