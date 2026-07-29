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
  const configuredScreens = parseArray(workspace.dataset.screenLayout);
  const resolutionOptions = parseArray(workspace.dataset.resolutionOptions);
  const canvas = document.createElement('canvas');
  const context = canvas.getContext('2d', { alpha: false });
  const video = document.createElement('video');
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
  let selectedScreenId = 'all';
  let autoScale = true;
  let pointerButtons = 0;
  const pressedKeys = new Map();
  let resolutionChanging = false;
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
    if (!peerConnection || videoFrameCallback !== null) {
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
    if (!controlChannel || controlChannel.readyState !== 'open') {
      return false;
    }
    controlChannel.send(JSON.stringify(command));
    return true;
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
    if (viewOnly) {
      return;
    }
    const point = pointerPosition(event);
    if (point) {
      send({ type: 'pointer', x: point.x, y: point.y, buttons });
    }
  }

  function sendPointerMove(event) {
    if (viewOnly) {
      return;
    }
    const point = pointerPosition(event);
    if (!point) {
      return;
    }
    const command = JSON.stringify({ type: 'pointer_move', x: point.x, y: point.y });
    if (pointerChannel && pointerChannel.readyState === 'open') {
      if (pointerChannel.bufferedAmount < 4096) {
        pointerChannel.send(command);
      }
      return;
    }
    send({ type: 'pointer_move', x: point.x, y: point.y });
  }

  function eventButtonMask(button) {
    if (button === 0) return 0x01;
    if (button === 1) return 0x02;
    if (button === 2) return 0x04;
    return 0;
  }

  function virtualKeyFor(event) {
    const code = event.code || '';
    if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3);
    if (/^Digit[0-9]$/.test(code)) return code.charCodeAt(5);
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return 111 + Number(code.slice(1));
    const keys = {
      Backspace: 8,
      Tab: 9,
      Enter: 13,
      NumpadEnter: 13,
      ShiftLeft: 16,
      ShiftRight: 16,
      ControlLeft: 17,
      ControlRight: 17,
      AltLeft: 18,
      AltRight: 18,
      Pause: 19,
      CapsLock: 20,
      Escape: 27,
      Space: 32,
      PageUp: 33,
      PageDown: 34,
      End: 35,
      Home: 36,
      ArrowLeft: 37,
      ArrowUp: 38,
      ArrowRight: 39,
      ArrowDown: 40,
      PrintScreen: 44,
      Insert: 45,
      Delete: 46,
      MetaLeft: 91,
      MetaRight: 92,
      ContextMenu: 93,
      Numpad0: 96,
      Numpad1: 97,
      Numpad2: 98,
      Numpad3: 99,
      Numpad4: 100,
      Numpad5: 101,
      Numpad6: 102,
      Numpad7: 103,
      Numpad8: 104,
      Numpad9: 105,
      NumpadMultiply: 106,
      NumpadAdd: 107,
      NumpadSubtract: 109,
      NumpadDecimal: 110,
      NumpadDivide: 111,
      NumLock: 144,
      ScrollLock: 145,
      Semicolon: 186,
      Equal: 187,
      Comma: 188,
      Minus: 189,
      Period: 190,
      Slash: 191,
      Backquote: 192,
      BracketLeft: 219,
      Backslash: 220,
      BracketRight: 221,
      Quote: 222,
    };
    return keys[code] || 0;
  }

  function isExtendedKey(code) {
    return (
      code === 'ControlRight' ||
      code === 'AltRight' ||
      code === 'NumpadEnter' ||
      code === 'NumpadDivide' ||
      code === 'Insert' ||
      code === 'Delete' ||
      code === 'Home' ||
      code === 'End' ||
      code === 'PageUp' ||
      code === 'PageDown' ||
      code.startsWith('Arrow') ||
      code.startsWith('Meta')
    );
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
    const nextWidth = Math.max(1, Number(message.width) || 1);
    const nextHeight = Math.max(1, Number(message.height) || 1);
    if (frameWidth === nextWidth && frameHeight === nextHeight) {
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
    } else if (message.type === 'error') {
      setStatus(message.message || 'Desktop stream failed', true);
    }
  }

  function closePeerConnection() {
    const currentPeer = peerConnection;
    peerConnection = null;
    controlChannel = null;
    pointerChannel = null;
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

  function waitForIceGatheringComplete(peer) {
    if (peer.iceGatheringState === 'complete') {
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      const timeout = window.setTimeout(finish, 10000);
      function finish() {
        window.clearTimeout(timeout);
        peer.removeEventListener('icegatheringstatechange', handleStateChange);
        resolve();
      }
      const handleStateChange = () => {
        if (peer.iceGatheringState === 'complete') {
          finish();
        }
      };
      peer.addEventListener('icegatheringstatechange', handleStateChange);
    });
  }

  async function startPeerConnection(iceServers) {
    if (peerConnection) {
      return;
    }
    if (typeof RTCPeerConnection !== 'function') {
      throw new Error('This browser does not support WebRTC');
    }

    const peer = new RTCPeerConnection({
      iceServers: Array.isArray(iceServers) ? iceServers : [],
    });
    peerConnection = peer;
    const channel = peer.createDataChannel('latitude-control', { ordered: true });
    controlChannel = channel;
    const pointer = peer.createDataChannel('latitude-pointer', {
      ordered: false,
      maxRetransmits: 0,
    });
    pointerChannel = pointer;
    pointer.addEventListener('close', () => {
      if (pointerChannel === pointer) {
        pointerChannel = null;
      }
    });
    channel.addEventListener('open', () => {
      if (controlChannel !== channel) return;
      setStatus('Connected');
      canvas.focus({ preventScroll: true });
    });
    channel.addEventListener('message', handleControlMessage);
    channel.addEventListener('close', () => {
      if (controlChannel === channel) {
        controlChannel = null;
      }
    });
    channel.addEventListener('error', () => {
      setStatus('Desktop control channel failed', true);
    });

    peer.addEventListener('track', (event) => {
      if (peerConnection !== peer || event.track.kind !== 'video') return;
      try {
        if ('playoutDelayHint' in event.receiver) {
          event.receiver.playoutDelayHint = 0;
        }
        if ('jitterBufferTarget' in event.receiver) {
          event.receiver.jitterBufferTarget = 0;
        }
      } catch (_) {
        // These low-latency hints are optional and browser-specific.
      }
      video.srcObject = event.streams[0] || new MediaStream([event.track]);
      void video.play().then(scheduleVideoFrame).catch((error) => {
        setStatus(error?.message || 'Desktop video could not start', true);
      });
    });
    peer.addEventListener('connectionstatechange', () => {
      if (peerConnection !== peer) return;
      if (peer.connectionState === 'connected') {
        reconnectDelay = 1000;
        setStatus('Connected');
      } else if (peer.connectionState === 'connecting') {
        setStatus('Connecting media');
      } else if (peer.connectionState === 'failed') {
        setStatus('WebRTC connection failed', true);
        socket?.close();
      }
    });

    peer.addTransceiver('video', { direction: 'recvonly' });
    await peer.setLocalDescription(await peer.createOffer());
    await waitForIceGatheringComplete(peer);
    if (peerConnection !== peer || !peer.localDescription) {
      return;
    }
    setStatus('Negotiating');
    sendSignal({ type: 'offer', sdp: peer.localDescription.sdp });
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
          await startPeerConnection(message.ice_servers);
        } catch (error) {
          setStatus(error?.message || 'WebRTC could not be started', true);
          nextSocket.close();
        }
      } else if (message.type === 'answer') {
        if (!peerConnection) return;
        try {
          await peerConnection.setRemoteDescription({ type: 'answer', sdp: message.sdp });
          setStatus('Connecting media');
        } catch (error) {
          setStatus(error?.message || 'WebRTC answer was rejected', true);
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
      closePeerConnection();
      scheduleReconnect();
    });
    nextSocket.addEventListener('error', () => {
      if (socket === nextSocket) setStatus('Desktop connection failed', true);
    });
  }

  canvas.addEventListener('pointerdown', (event) => {
    if (viewOnly) return;
    event.preventDefault();
    canvas.focus({ preventScroll: true });
    canvas.setPointerCapture?.(event.pointerId);
    pointerButtons |= eventButtonMask(event.button);
    sendPointer(event);
  });
  canvas.addEventListener('pointermove', (event) => {
    if (viewOnly) return;
    event.preventDefault();
    sendPointerMove(event);
  });
  const releasePointer = (event) => {
    if (viewOnly) return;
    event.preventDefault();
    pointerButtons &= ~eventButtonMask(event.button);
    sendPointer(event);
  };
  canvas.addEventListener('pointerup', releasePointer);
  canvas.addEventListener('pointercancel', releasePointer);
  canvas.addEventListener('contextmenu', (event) => event.preventDefault());
  canvas.addEventListener(
    'wheel',
    (event) => {
      if (viewOnly) return;
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
    if (viewOnly) return;
    const vk = virtualKeyFor(event);
    if (!vk) return;
    event.preventDefault();
    const extended = isExtendedKey(event.code || '');
    pressedKeys.set(event.code || `${vk}:${extended}`, { vk, extended });
    send({ type: 'key', vk, down: true, extended });
  });
  canvas.addEventListener('keyup', (event) => {
    if (viewOnly) return;
    const vk = virtualKeyFor(event);
    if (!vk) return;
    event.preventDefault();
    const extended = isExtendedKey(event.code || '');
    pressedKeys.delete(event.code || `${vk}:${extended}`);
    send({ type: 'key', vk, down: false, extended });
  });
  const releasePressedKeys = () => {
    if (viewOnly) return;
    pressedKeys.clear();
    send({ type: 'release_keys' });
  };
  canvas.addEventListener('blur', releasePressedKeys);
  canvas.addEventListener('paste', (event) => {
    if (viewOnly) return;
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
      const response = await fetch(actionPath, {
        method: 'PATCH',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          action: 'set_resolution',
          screen_id: selectedScreenId,
          width,
          height,
        }),
      });
      if (!response.ok) {
        const payload = await response.json().catch(() => ({}));
        throw new Error(payload.error || `Resolution change failed (${response.status})`);
      }
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
  window.addEventListener('blur', releasePressedKeys);
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
    releasePressedKeys();
    socket?.close();
    closePeerConnection();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      releasePressedKeys();
    }
  });

  renderResolutionOptions();
  renderScreenSwitcher();
  updateScaleButton();
  updateFullscreenButton();
  connect();
}
