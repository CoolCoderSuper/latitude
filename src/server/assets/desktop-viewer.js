import RFB from 'https://cdn.jsdelivr.net/npm/@novnc/novnc@1.7.0/core/rfb.js';

const workspace = document.querySelector('[data-desktop-workspace]');

if (workspace) {
  const target = workspace.querySelector('[data-desktop-target]');
  const status = workspace.querySelector('[data-desktop-status]');
  const screenSwitcher = workspace.querySelector('[data-desktop-screens]');
  const resolutionSelect = workspace.querySelector('[data-desktop-resolution]');
  const scaleButton = workspace.querySelector('[data-desktop-scale]');
  const fullscreenButton = workspace.querySelector('[data-desktop-fullscreen]');
  const credentials = workspace.querySelector('[data-desktop-credentials]');
  const credentialUser = workspace.querySelector('[data-desktop-credential-user]');
  const credentialPassword = workspace.querySelector('[data-desktop-credential-password]');
  const credentialTarget = workspace.querySelector('[data-desktop-credential-target]');
  const viewOnly = workspace.dataset.viewOnly !== 'false';
  let rfb = null;
  let reconnectTimer = null;
  let reconnectDelay = 1000;
  let reconnectEnabled = true;
  let selectedScreenId = 'all';
  let screenOptions = [];
  let screenRefreshTimer = null;
  let resizeObserver = null;
  let autoScale = true;
  let lastAppliedViewport = '';
  let layoutRetryTimers = [];
  let fullRefreshTimers = [];
  let configuredScreens = [];
  let resolutionOptions = [];
  let resolutionChanging = false;
  let lastSentClipboardText = null;
  let interceptedPasteKey = false;
  let suppressPasteKeyUp = false;
  let pasteFocusTarget = null;

  const setStatus = (message, isError = false) => {
    if (!status) {
      return;
    }

    status.textContent = message;
    status.classList.toggle('error', Boolean(isError));
  };

  const writeLocalClipboard = async (text) => {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }

    const activeElement = document.activeElement;
    const bridge = document.createElement('textarea');
    bridge.className = 'desktop-clipboard-bridge';
    bridge.tabIndex = -1;
    bridge.setAttribute('aria-hidden', 'true');
    bridge.value = text;
    document.body.appendChild(bridge);
    bridge.focus({ preventScroll: true });
    bridge.select();
    bridge.setSelectionRange(0, bridge.value.length);
    const copied = Boolean(document.execCommand?.('copy'));
    bridge.remove();
    activeElement?.focus?.({ preventScroll: true });
    if (!copied) {
      throw new Error('Clipboard access is unavailable; select and copy the text manually.');
    }
  };

  const sendRemoteClipboard = (text) => {
    if (viewOnly || !rfb || typeof rfb.clipboardPasteFrom !== 'function') {
      return false;
    }

    rfb.clipboardPasteFrom(text);
    lastSentClipboardText = text;
    return true;
  };

  const sendRemotePasteShortcut = () => {
    if (viewOnly || !rfb || typeof rfb.sendKey !== 'function') {
      return;
    }

    rfb.sendKey(0xffe3, 'ControlLeft', true);
    rfb.sendKey(0x0076, 'KeyV', true);
    rfb.sendKey(0x0076, 'KeyV', false);
    rfb.sendKey(0xffe3, 'ControlLeft', false);
  };

  const syncLocalClipboardToRemote = async () => {
    if (viewOnly || !rfb || !navigator.clipboard?.readText) {
      return;
    }

    try {
      const text = await navigator.clipboard.readText();
      if (text !== lastSentClipboardText) {
        sendRemoteClipboard(text);
      }
    } catch (_) {
      // Browsers can reject background clipboard reads. A paste event or the
      // explicit clipboard panel remains available in that case.
    }
  };

  const buildSocketUrl = () => {
    const url = new URL(workspace.dataset.wsPath || `${window.location.pathname.replace(/\/$/, '')}/ws`, window.location.href);
    url.protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    if (workspace.dataset.wsToken) {
      url.searchParams.set('token', workspace.dataset.wsToken);
    }
    return url;
  };

  const clearReconnectTimer = () => {
    if (reconnectTimer) {
      window.clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };

  const hideCredentials = () => {
    if (credentials) {
      credentials.hidden = true;
    }
  };

  const showCredentials = (types) => {
    if (!credentials) {
      return;
    }

    const required = new Set(types || ['password']);
    credentialUser.hidden = !required.has('username');
    credentialPassword.hidden = !required.has('password');
    credentialTarget.hidden = !required.has('target');
    credentials.hidden = false;
    const firstInput = credentials.querySelector('label:not([hidden]) input');
    if (firstInput) {
      firstInput.focus();
    }
  };

  const collectCredentials = () => {
    const payload = {};
    const username = credentialUser?.querySelector('input')?.value || '';
    const password = credentialPassword?.querySelector('input')?.value || '';
    const targetValue = credentialTarget?.querySelector('input')?.value || '';
    if (username) {
      payload.username = username;
    }
    if (password) {
      payload.password = password;
    }
    if (targetValue) {
      payload.target = targetValue;
    }
    return payload;
  };

  const scheduleReconnect = () => {
    if (!reconnectEnabled || reconnectTimer) {
      return;
    }

    const delay = reconnectDelay;
    setStatus('Reconnecting', true);
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
    reconnectDelay = Math.min(8000, Math.floor(reconnectDelay * 1.6));
  };

  const updateScaleButton = () => {
    if (!scaleButton) {
      return;
    }

    scaleButton.textContent = autoScale ? 'Fit' : '1:1';
    scaleButton.classList.toggle('active', autoScale);
    scaleButton.setAttribute('aria-pressed', String(autoScale));
    scaleButton.title = autoScale ? 'Auto-scale is on' : 'Auto-scale is off';
  };

  const updateFullscreenButton = () => {
    if (!fullscreenButton) {
      return;
    }

    const isFullscreen = document.fullscreenElement === workspace;
    fullscreenButton.textContent = isFullscreen ? 'Exit' : 'Full';
    fullscreenButton.classList.toggle('active', isFullscreen);
    fullscreenButton.setAttribute('aria-pressed', String(isFullscreen));
  };

  const displayFor = (currentRfb) => currentRfb?._display || null;

  const requestFullFramebufferUpdate = () => {
    if (!rfb?._sock || !rfb._fbWidth || !rfb._fbHeight) {
      return;
    }

    try {
      RFB.messages.fbUpdateRequest(rfb._sock, false, 0, 0, rfb._fbWidth, rfb._fbHeight);
    } catch (_) {}
  };

  const clearFullRefreshTimers = () => {
    for (const timer of fullRefreshTimers) {
      window.clearTimeout(timer);
    }
    fullRefreshTimers = [];
  };

  const scheduleFullFramebufferRefresh = () => {
    clearFullRefreshTimers();
    requestFullFramebufferUpdate();

    for (const delay of [120, 300, 700, 1400, 2400]) {
      fullRefreshTimers.push(window.setTimeout(requestFullFramebufferUpdate, delay));
    }
  };

  const expectedScaleFor = (screen, width, height) => {
    if (!autoScale || !screen.width || !screen.height) {
      return 1;
    }

    return Math.min(width / screen.width, height / screen.height);
  };

  const layoutMatches = (display, screen, width, height) => {
    const expectedScale = expectedScaleFor(screen, width, height);
    const actualScale = Number(display.scale || 0);
    const expectedClip = screen.id !== 'all';
    const actualClip = Boolean(display.clipViewport);
    const viewport = display._viewportLoc || {};
    const viewportMatches =
      Math.abs(Number(viewport.x || 0) - screen.x) <= 1 &&
      Math.abs(Number(viewport.y || 0) - screen.y) <= 1 &&
      Math.abs(Number(viewport.w || 0) - screen.width) <= 1 &&
      Math.abs(Number(viewport.h || 0) - screen.height) <= 1;

    return (
      Math.abs(actualScale - expectedScale) < 0.002 &&
      actualClip === expectedClip &&
      viewportMatches
    );
  };

  const displaySize = (currentRfb) => {
    const display = displayFor(currentRfb);
    const width = Number(display?.width || currentRfb?._fbWidth || 0);
    const height = Number(display?.height || currentRfb?._fbHeight || 0);
    return { width, height };
  };

  const normalizedInteger = (value) => {
    const number = Number(value);
    if (!Number.isFinite(number)) {
      return 0;
    }
    return Math.max(0, Math.floor(number));
  };

  const normalizeScreenLayout = (value) => {
    if (!Array.isArray(value)) {
      return [];
    }

    return value
      .map((screen, index) => {
        const label = String(screen?.label || index + 1);
        return {
          id: String(screen?.id || `display-${index + 1}`),
          label,
          title: String(screen?.title || `Screen ${label}`),
          x: normalizedInteger(screen?.x),
          y: normalizedInteger(screen?.y),
          width: normalizedInteger(screen?.width),
          height: normalizedInteger(screen?.height),
          primary: Boolean(screen?.primary),
        };
      })
      .filter((screen) => screen.width > 0 && screen.height > 0);
  };

  const parseConfiguredScreens = () => {
    try {
      return normalizeScreenLayout(JSON.parse(workspace.dataset.screenLayout || '[]'));
    } catch (_) {
      return [];
    }
  };

  const commonResolutionOptions = [
    { width: 1024, height: 768 },
    { width: 1280, height: 720 },
    { width: 1280, height: 800 },
    { width: 1366, height: 768 },
    { width: 1440, height: 900 },
    { width: 1600, height: 900 },
    { width: 1920, height: 1080 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ];

  const normalizeResolutionOptions = (value) => {
    if (!Array.isArray(value)) {
      return [];
    }

    const seen = new Set();
    return value
      .map((resolution) => ({
        width: normalizedInteger(resolution?.width),
        height: normalizedInteger(resolution?.height),
        current: Boolean(resolution?.current),
      }))
      .filter((resolution) => {
        if (resolution.width < 640 || resolution.height < 480) {
          return false;
        }

        const key = `${resolution.width}x${resolution.height}`;
        if (seen.has(key)) {
          return false;
        }
        seen.add(key);
        return true;
      })
      .sort((left, right) => {
        const leftPixels = left.width * left.height;
        const rightPixels = right.width * right.height;
        return leftPixels - rightPixels || left.width - right.width || left.height - right.height;
      });
  };

  const parseResolutionOptions = () => {
    try {
      return normalizeResolutionOptions(JSON.parse(workspace.dataset.resolutionOptions || '[]'));
    } catch (_) {
      return [];
    }
  };

  configuredScreens = parseConfiguredScreens();
  resolutionOptions = parseResolutionOptions();

  const configuredScreensFor = (width, height) => {
    if (configuredScreens.length < 2) {
      return [];
    }

    return configuredScreens
      .map((screen) => {
        if (screen.x >= width || screen.y >= height) {
          return null;
        }

        const clippedWidth = Math.min(screen.width, width - screen.x);
        const clippedHeight = Math.min(screen.height, height - screen.y);
        if (clippedWidth <= 0 || clippedHeight <= 0) {
          return null;
        }

        return {
          ...screen,
          width: clippedWidth,
          height: clippedHeight,
        };
      })
      .filter(Boolean);
  };

  const splitHorizontal = (width, height, count) =>
    Array.from({ length: count }, (_, index) => {
      const x = Math.floor((width * index) / count);
      const nextX = Math.floor((width * (index + 1)) / count);
      return {
        id: `screen-${index + 1}`,
        label: String(index + 1),
        title: `Screen ${index + 1}`,
        x,
        y: 0,
        width: nextX - x,
        height,
      };
    });

  const splitVertical = (width, height, count) =>
    Array.from({ length: count }, (_, index) => {
      const y = Math.floor((height * index) / count);
      const nextY = Math.floor((height * (index + 1)) / count);
      return {
        id: `screen-${index + 1}`,
        label: String(index + 1),
        title: `Screen ${index + 1}`,
        x: 0,
        y,
        width,
        height: nextY - y,
      };
    });

  const detectedScreens = (width, height) => {
    if (!width || !height) {
      return [];
    }

    const standardAspect = 16 / 9;
    const aspect = width / height;
    const tallAspect = height / width;

    if (aspect >= 2.35) {
      const count = Math.min(4, Math.max(2, Math.round(aspect / standardAspect)));
      return splitHorizontal(width, height, count);
    }

    if (tallAspect >= 2.35) {
      const count = Math.min(4, Math.max(2, Math.round(tallAspect / standardAspect)));
      return splitVertical(width, height, count);
    }

    return [];
  };

  const buildScreenOptions = () => {
    const { width, height } = displaySize(rfb);
    if (!width || !height) {
      return [];
    }

    const allScreens = {
      id: 'all',
      label: 'All',
      title: 'All screens',
      x: 0,
      y: 0,
      width,
      height,
    };
    let screens = configuredScreensFor(width, height);
    if (screens.length < 2) {
      screens = detectedScreens(width, height);
    }
    if (screens.length < 2) {
      return [allScreens];
    }

    return [allScreens, ...screens];
  };

  const sameScreenOptions = (left, right) => {
    if (left.length !== right.length) {
      return false;
    }

    return left.every((screen, index) => {
      const other = right[index];
      return (
        screen.id === other.id &&
        screen.label === other.label &&
        screen.x === other.x &&
        screen.y === other.y &&
        screen.width === other.width &&
        screen.height === other.height &&
        screen.primary === other.primary
      );
    });
  };

  const renderScreenSwitcher = () => {
    if (!screenSwitcher) {
      return;
    }

    screenSwitcher.hidden = screenOptions.length < 2;
    screenSwitcher.replaceChildren();
    if (screenOptions.length < 2) {
      return;
    }

    for (const screen of screenOptions) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = screen.label;
      button.title = screen.title;
      button.setAttribute('aria-label', screen.title);
      button.classList.toggle('active', screen.id === selectedScreenId);
      button.addEventListener('click', () => {
        selectedScreenId = screen.id;
        lastAppliedViewport = '';
        renderScreenSwitcher();
        renderResolutionSelect();
        applySelectedScreen(true);
        scheduleLayoutRetry();
      });
      screenSwitcher.appendChild(button);
    }
  };

  const selectedScreen = () =>
    screenOptions.find((screen) => screen.id === selectedScreenId) || screenOptions[0] || null;

  const resolutionKey = (resolution) => `${resolution.width}x${resolution.height}`;

  const currentResolutionForSelect = () => {
    const screen = selectedScreen();
    if (screen?.id && screen.id !== 'all' && screen.width && screen.height) {
      return { width: screen.width, height: screen.height };
    }

    const size = displaySize(rfb);
    if (size.width && size.height) {
      return { width: Math.round(size.width), height: Math.round(size.height) };
    }

    return resolutionOptions.find((resolution) => resolution.current) || null;
  };

  const availableResolutionOptions = () => {
    const current = currentResolutionForSelect();
    const baseOptions = resolutionOptions.length > 0 ? resolutionOptions : commonResolutionOptions;
    const seen = new Set();
    const options = [];

    for (const resolution of [...baseOptions, current].filter(Boolean)) {
      const width = normalizedInteger(resolution.width);
      const height = normalizedInteger(resolution.height);
      if (width < 640 || height < 480) {
        continue;
      }

      const key = `${width}x${height}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      options.push({ width, height });
    }

    return options.sort((left, right) => {
      const leftPixels = left.width * left.height;
      const rightPixels = right.width * right.height;
      return leftPixels - rightPixels || left.width - right.width || left.height - right.height;
    });
  };

  const renderResolutionSelect = () => {
    if (!resolutionSelect) {
      return;
    }

    const options = availableResolutionOptions();
    const current = currentResolutionForSelect();
    const currentKey = current ? resolutionKey(current) : '';
    resolutionSelect.hidden = options.length === 0;
    resolutionSelect.disabled = resolutionChanging || options.length === 0;
    resolutionSelect.replaceChildren();

    for (const resolution of options) {
      const option = document.createElement('option');
      option.value = resolutionKey(resolution);
      option.textContent = `${resolution.width} x ${resolution.height}`;
      option.selected = option.value === currentKey;
      resolutionSelect.appendChild(option);
    }
  };

  const applySelectedScreen = (force = false) => {
    const display = displayFor(rfb);
    const screen = selectedScreen();
    if (!target || !display || !screen) {
      return;
    }

    const bounds = target.getBoundingClientRect();
    const width = Math.max(1, bounds.width);
    const height = Math.max(1, bounds.height);
    const displayWidth = Number(display.width || 0);
    const displayHeight = Number(display.height || 0);
    const layoutKey = [
      screen.id,
      screen.x,
      screen.y,
      screen.width,
      screen.height,
      displayWidth,
      displayHeight,
      Math.round(width),
      Math.round(height),
      autoScale ? 'fit' : 'native',
    ].join(':');

    if (!force && layoutKey === lastAppliedViewport && layoutMatches(display, screen, width, height)) {
      return;
    }
    lastAppliedViewport = layoutKey;

    if (rfb?._screen) {
      rfb._screen.style.overflow = autoScale ? 'hidden' : 'auto';
    }

    if (screen.id === 'all') {
      rfb.clipViewport = false;
      rfb.scaleViewport = autoScale;
      display.clipViewport = false;
      display.viewportChangeSize();
    } else {
      rfb.scaleViewport = false;
      rfb.clipViewport = true;
      display.clipViewport = true;
      display.viewportChangeSize(screen.width, screen.height);
      const viewport = display._viewportLoc || { x: 0, y: 0 };
      display.viewportChangePos(screen.x - viewport.x, screen.y - viewport.y);
    }

    if (autoScale) {
      display.autoscale(width, height);
    } else {
      display.scale = 1;
    }

    if (force) {
      scheduleFullFramebufferRefresh();
    }
  };

  const refreshScreenOptions = () => {
    const nextOptions = buildScreenOptions();
    const selectedExists = nextOptions.some((screen) => screen.id === selectedScreenId);
    let selectedChanged = false;
    if (!selectedExists) {
      selectedScreenId = 'all';
      selectedChanged = true;
    }

    if (!sameScreenOptions(screenOptions, nextOptions)) {
      screenOptions = nextOptions;
      lastAppliedViewport = '';
      renderScreenSwitcher();
    } else if (selectedChanged) {
      lastAppliedViewport = '';
      renderScreenSwitcher();
    }

    applySelectedScreen();
    renderResolutionSelect();
  };

  const clearLayoutRetries = () => {
    for (const timer of layoutRetryTimers) {
      window.clearTimeout(timer);
    }
    layoutRetryTimers = [];
  };

  const scheduleLayoutRetry = () => {
    clearLayoutRetries();

    const retry = () => {
      lastAppliedViewport = '';
      refreshScreenOptions();
    };

    window.requestAnimationFrame(() => {
      retry();
      window.requestAnimationFrame(retry);
    });

    for (const delay of [80, 180, 360, 720]) {
      layoutRetryTimers.push(window.setTimeout(retry, delay));
    }
  };

  const stopScreenRefresh = () => {
    clearLayoutRetries();
    clearFullRefreshTimers();
    if (screenRefreshTimer) {
      window.clearInterval(screenRefreshTimer);
      screenRefreshTimer = null;
    }
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    screenOptions = [];
    selectedScreenId = 'all';
    lastAppliedViewport = '';
    renderScreenSwitcher();
    renderResolutionSelect();
  };

  const startScreenRefresh = () => {
    stopScreenRefresh();
    if (!target) {
      return;
    }

    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(refreshScreenOptions);
      resizeObserver.observe(target);
    }
    screenRefreshTimer = window.setInterval(refreshScreenOptions, 1000);
    window.setTimeout(refreshScreenOptions, 50);
    window.setTimeout(refreshScreenOptions, 500);
    scheduleLayoutRetry();
  };

  const desktopActionUrl = () => new URL(workspace.dataset.actionPath || window.location.pathname, window.location.href);

  const parseResolutionValue = (value) => {
    const match = /^(\d+)x(\d+)$/.exec(String(value || ''));
    if (!match) {
      return null;
    }

    return {
      width: Number(match[1]),
      height: Number(match[2]),
    };
  };

  const refreshResolutionData = (payload) => {
    if (Array.isArray(payload?.screens)) {
      configuredScreens = normalizeScreenLayout(payload.screens);
      workspace.dataset.screenLayout = JSON.stringify(configuredScreens);
    }
    if (Array.isArray(payload?.resolutions)) {
      resolutionOptions = normalizeResolutionOptions(payload.resolutions);
      workspace.dataset.resolutionOptions = JSON.stringify(resolutionOptions);
    }
  };

  const applyResolution = async (resolution) => {
    if (!resolution || resolutionChanging) {
      return;
    }

    const current = currentResolutionForSelect();
    if (current && current.width === resolution.width && current.height === resolution.height) {
      renderResolutionSelect();
      return;
    }

    resolutionChanging = true;
    renderResolutionSelect();
    setStatus('Changing resolution');

    try {
      const headers = { 'content-type': 'application/json' };
      if (workspace.dataset.wsToken) {
        headers.authorization = `Bearer ${workspace.dataset.wsToken}`;
      }
      const response = await fetch(desktopActionUrl(), {
        method: 'PATCH',
        credentials: 'same-origin',
        headers,
        body: JSON.stringify({
          action: 'set_resolution',
          width: resolution.width,
          height: resolution.height,
          screen_id: selectedScreenId.startsWith('display-') ? selectedScreenId : null,
        }),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(payload.error || 'Resolution change failed');
      }

      refreshResolutionData(payload);
      lastAppliedViewport = '';
      setStatus('Resolution changed');
      window.latitudeReconnect?.(true);
      scheduleLayoutRetry();
      window.setTimeout(scheduleLayoutRetry, 600);
      window.setTimeout(scheduleFullFramebufferRefresh, 1000);
      window.setTimeout(() => {
        setStatus('');
      }, 1400);
    } catch (error) {
      setStatus(error.message || 'Resolution change failed', true);
    } finally {
      resolutionChanging = false;
      renderResolutionSelect();
    }
  };

  resolutionSelect?.addEventListener('change', () => {
    applyResolution(parseResolutionValue(resolutionSelect.value));
  });

  document.addEventListener('paste', (event) => {
    if (viewOnly) {
      return;
    }

    const text = event.clipboardData?.getData('text/plain');
    if (typeof text !== 'string') {
      return;
    }

    try {
      if (sendRemoteClipboard(text)) {
        if (interceptedPasteKey) {
          event.preventDefault();
          pasteFocusTarget?.focus?.({ preventScroll: true });
          window.setTimeout(sendRemotePasteShortcut, 25);
        }
      }
    } catch (_) {}
    interceptedPasteKey = false;
    pasteFocusTarget = null;
  }, true);

  window.addEventListener('keydown', (event) => {
    const activeElement = document.activeElement;
    const viewerHasFocus = Boolean(target && (activeElement === target || target.contains(activeElement)));
    if (
      viewOnly ||
      !viewerHasFocus ||
      event.altKey ||
      (!event.ctrlKey && !event.metaKey) ||
      event.key.toLowerCase() !== 'v'
    ) {
      return;
    }

    interceptedPasteKey = true;
    suppressPasteKeyUp = true;
    pasteFocusTarget = activeElement;
    const bridge = document.createElement('textarea');
    bridge.className = 'desktop-clipboard-bridge';
    bridge.tabIndex = -1;
    bridge.setAttribute('aria-hidden', 'true');
    bridge.addEventListener('paste', () => window.setTimeout(() => bridge.remove(), 0), { once: true });
    document.body.appendChild(bridge);
    bridge.focus({ preventScroll: true });
    event.stopImmediatePropagation();
    window.setTimeout(() => {
      bridge.remove();
      interceptedPasteKey = false;
      pasteFocusTarget = null;
    }, 1000);
  }, true);

  window.addEventListener('keyup', (event) => {
    if (suppressPasteKeyUp && event.key.toLowerCase() === 'v') {
      suppressPasteKeyUp = false;
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }, true);

  scaleButton?.addEventListener('click', () => {
    autoScale = !autoScale;
    lastAppliedViewport = '';
    updateScaleButton();
    scheduleLayoutRetry();
  });

  fullscreenButton?.addEventListener('click', async () => {
    try {
      if (document.fullscreenElement === workspace) {
        await document.exitFullscreen();
      } else {
        await workspace.requestFullscreen();
      }
    } catch (error) {
      setStatus(error.message || 'Fullscreen unavailable', true);
    }
  });

  document.addEventListener('fullscreenchange', () => {
    updateFullscreenButton();
    lastAppliedViewport = '';
    scheduleLayoutRetry();
  });

  const configureRfb = (nextRfb) => {
    nextRfb.viewOnly = viewOnly;
    nextRfb.scaleViewport = false;
    nextRfb.resizeSession = false;
    nextRfb.clipViewport = false;
    nextRfb.dragViewport = false;
    nextRfb.focusOnClick = !viewOnly;
    nextRfb.qualityLevel = 6;
    nextRfb.compressionLevel = 2;

    nextRfb.addEventListener('connect', () => {
      clearReconnectTimer();
      reconnectDelay = 1000;
      hideCredentials();
      setStatus('Connected');
      startScreenRefresh();
      scheduleFullFramebufferRefresh();
      syncLocalClipboardToRemote();
      window.setTimeout(() => {
        if (rfb === nextRfb) {
          setStatus('');
        }
      }, 1000);
    });

    nextRfb.addEventListener('disconnect', (event) => {
      if (rfb !== nextRfb) {
        return;
      }

      rfb = null;
      lastSentClipboardText = null;
      stopScreenRefresh();
      if (event.detail.clean) {
        setStatus('Disconnected', true);
      } else {
        scheduleReconnect();
      }
    });

    nextRfb.addEventListener('credentialsrequired', (event) => {
      showCredentials(event.detail.types);
      setStatus('Credentials required', true);
    });

    nextRfb.addEventListener('securityfailure', (event) => {
      setStatus(event.detail.reason || 'Security failure', true);
    });

    nextRfb.addEventListener('clipboard', (event) => {
      const text = event.detail?.text || '';
      lastSentClipboardText = text;
      writeLocalClipboard(text).catch(() => {});
    });
  };

  const connect = () => {
    if (!target || rfb) {
      return;
    }

    clearReconnectTimer();
    hideCredentials();
    target.replaceChildren();
    setStatus('Connecting');
    try {
      const nextRfb = new RFB(target, buildSocketUrl().toString(), { shared: true });
      rfb = nextRfb;
      configureRfb(nextRfb);
    } catch (error) {
      setStatus(error.message || 'Connection failed', true);
      scheduleReconnect();
    }
  };

  const reconnect = (force) => {
    clearReconnectTimer();
    reconnectDelay = 1000;
    if (force && rfb) {
      const current = rfb;
      rfb = null;
      try {
        current.disconnect();
      } catch (_) {}
    }
    if (!rfb) {
      connect();
    }
  };

  window.latitudeReconnect = reconnect;

  credentials?.addEventListener('submit', (event) => {
    event.preventDefault();
    if (!rfb) {
      connect();
      return;
    }

    rfb.sendCredentials(collectCredentials());
    hideCredentials();
    setStatus('Authenticating');
  });

  window.addEventListener('focus', () => {
    reconnect(false);
    syncLocalClipboardToRemote();
  });
  window.addEventListener('online', () => reconnect(true));
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
      reconnect(false);
      syncLocalClipboardToRemote();
    }
  });
  window.addEventListener('beforeunload', () => {
    reconnectEnabled = false;
    clearReconnectTimer();
    stopScreenRefresh();
    if (rfb) {
      rfb.disconnect();
    }
  });

  updateScaleButton();
  updateFullscreenButton();
  renderResolutionSelect();

  if (target) {
    connect();
  } else {
    setStatus('Desktop surface missing', true);
  }
}
