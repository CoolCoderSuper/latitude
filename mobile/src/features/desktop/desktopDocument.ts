import type { DesktopScreen } from '../../types';

export function desktopDocument(
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
      height: 100%;
      margin: 0;
      overflow: hidden;
      background: ${viewerBackgroundCss};
      color: #edf4f1;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    body {
      box-sizing: border-box;
      display: flex;
      flex-direction: column;
      padding: 0;
      overscroll-behavior: none;
      user-select: none;
      -webkit-touch-callout: none;
      -webkit-user-select: none;
    }

    .desktop-stage {
      position: relative;
      flex: 1 1 auto;
      min-height: 0;
    }

    #desktop {
      width: 100%;
      height: 100%;
      min-height: 0;
      overflow: hidden;
      background: ${viewerBackgroundCss};
      box-sizing: border-box;
      touch-action: none;
      user-select: none;
      -webkit-user-select: none;
    }

    #desktop canvas {
      background: transparent !important;
      touch-action: none;
      user-select: none;
      -webkit-user-select: none;
    }

    #desktop > div,
    #desktop .noVNC_screen,
    #desktop .noVNC_container {
      background: ${viewerBackgroundCss} !important;
    }

    .touch-cursor {
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

    .touch-cursor::after {
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

    .touch-cursor[hidden] {
      display: none;
    }
  </style>
</head>
<body>
  <div class="desktop-stage">
    <div id="desktop" tabindex="0"></div>
    <div class="touch-cursor" hidden></div>
  </div>
  <script>
    window.latitudeViewerStarted = false;
    window.latitudeIsBenignViewerError = function (message) {
      return String(message || '').indexOf('ResizeObserver loop') !== -1;
    };
    window.addEventListener('error', function (event) {
      if (window.latitudeIsBenignViewerError(event.message)) {
        return;
      }
      if (!window.latitudeViewerStarted && window.ReactNativeWebView) {
        window.ReactNativeWebView.postMessage(JSON.stringify({
          type: 'desktop-state',
          state: {
            status: event.message || 'Viewer script failed',
            statusIsError: true,
          },
        }));
      }
    });
    window.setTimeout(function () {
      if (!window.latitudeViewerStarted && window.ReactNativeWebView) {
        window.ReactNativeWebView.postMessage(JSON.stringify({
          type: 'desktop-state',
          state: {
            status: 'Viewer script failed',
            statusIsError: true,
          },
        }));
      }
    }, 3500);
  </script>
  <script type="module">
    import RFB from 'https://cdn.jsdelivr.net/npm/@novnc/novnc@1.7.0/core/rfb.js';

    window.latitudeViewerStarted = true;
    document.title = ${labelJson};
    const websocketUrl = ${websocketUrlJson};
    const viewOnly = ${viewOnlyJson};
    const configuredScreenLayout = ${screenLayoutJson};
    const viewerBackground = ${viewerBackgroundJson};
    const target = document.getElementById('desktop');
    const touchCursor = document.querySelector('.touch-cursor');
    const desktopStage = document.querySelector('.desktop-stage');

    let rfb = null;
    let reconnectTimer = null;
    let reconnectDelay = 1000;
    let reconnectEnabled = true;
    let selectedScreenId = 'all';
    let screenOptions = [];
    let screenRefreshTimer = null;
    let resizeObserver = null;
    let resizeObserverFrame = null;
    let autoScale = true;
    let zoomLevel = 1;
    let keyboardActive = false;
    let hasUserSelectedScreen = false;
    let lastAppliedViewport = '';
    let layoutRetryTimers = [];
    let fullRefreshTimers = [];
    let pointerMode = 'touchpad';
    let activeMouseButton = 0x1;
    let dragLocked = false;
    let touchPointer = null;
    let touchpadState = null;
    let pressedModifiers = new Set();
    let longPressTimer = null;
    let pendingPointerFrame = null;
    let pendingPointerMask = 0;
    const touchpadSpeed = 1.32;
    const tapMoveThreshold = 8;
    const longPressDelay = 640;
    const touchWheelStep = 46;
    const minZoom = 1;
    const maxZoom = 3;
    const zoomStep = 1.25;
    const pinchZoomThreshold = 12;
    const nativeState = {
      ready: false,
      connected: false,
      status: 'Connecting',
      statusIsError: false,
      viewOnly,
      autoScale,
      zoomLevel,
      selectedScreenId,
      screens: [],
      pointerMode,
      activeMouseButton,
      dragLocked,
      pressedModifiers: [],
      credentialsRequired: null,
    };
    let nativeStateTimer = null;

    const postNativeMessage = (payload) => {
      if (
        !window.ReactNativeWebView ||
        typeof window.ReactNativeWebView.postMessage !== 'function'
      ) {
        return;
      }

      try {
        window.ReactNativeWebView.postMessage(JSON.stringify(payload));
      } catch (_) {}
    };

    const flushNativeState = () => {
      nativeStateTimer = null;
      postNativeMessage({
        type: 'desktop-state',
        state: nativeState,
      });
    };

    const updateNativeState = (patch) => {
      Object.assign(nativeState, patch);
      if (nativeStateTimer) {
        return;
      }

      nativeStateTimer = window.setTimeout(flushNativeState, 0);
    };

    const setStatus = (text, isError = false) => {
      updateNativeState({
        status: text,
        statusIsError: Boolean(isError),
      });
    };

    const applyViewerBackground = (currentRfb = rfb) => {
      document.documentElement.style.background = viewerBackground;
      document.body.style.background = viewerBackground;
      target.style.background = viewerBackground;

      if (desktopStage) {
        desktopStage.style.background = viewerBackground;
      }
      if (currentRfb && currentRfb._screen) {
        currentRfb._screen.style.setProperty('background', viewerBackground, 'important');
        currentRfb._screen.style.setProperty('background-color', viewerBackground, 'important');
      }
      if (currentRfb && currentRfb._canvas) {
        currentRfb._canvas.style.setProperty('background', 'transparent', 'important');
        currentRfb._canvas.style.setProperty('background-color', 'transparent', 'important');
      }
    };

    const isBenignViewerError = (message) => {
      if (typeof window.latitudeIsBenignViewerError === 'function') {
        return window.latitudeIsBenignViewerError(message);
      }

      return String(message || '').includes('ResizeObserver loop');
    };

    window.addEventListener('error', (event) => {
      if (!isBenignViewerError(event.message)) {
        setStatus(event.message || 'Viewer error', true);
      }
    });

    window.addEventListener('unhandledrejection', (event) => {
      const reason = event.reason;
      const message = (reason && reason.message) || reason || 'Viewer error';
      if (!isBenignViewerError(message)) {
        setStatus(message, true);
      }
    });

    const clearReconnectTimer = () => {
      if (reconnectTimer) {
        window.clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const hideCredentials = () => {
      updateNativeState({ credentialsRequired: null });
    };

    const showCredentials = (types) => {
      const required = new Set(types || ['password']);
      updateNativeState({ credentialsRequired: Array.from(required) });
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
      updateNativeState({ autoScale });
    };

    const updateZoomControls = () => {
      updateNativeState({ zoomLevel });
    };

    const keyDefinitions = {
      backspace: { keysym: 0xff08, code: 'Backspace' },
      tab: { keysym: 0xff09, code: 'Tab' },
      enter: { keysym: 0xff0d, code: 'Enter' },
      escape: { keysym: 0xff1b, code: 'Escape' },
      delete: { keysym: 0xffff, code: 'Delete' },
      home: { keysym: 0xff50, code: 'Home' },
      left: { keysym: 0xff51, code: 'ArrowLeft' },
      up: { keysym: 0xff52, code: 'ArrowUp' },
      right: { keysym: 0xff53, code: 'ArrowRight' },
      down: { keysym: 0xff54, code: 'ArrowDown' },
      pageup: { keysym: 0xff55, code: 'PageUp' },
      pagedown: { keysym: 0xff56, code: 'PageDown' },
      end: { keysym: 0xff57, code: 'End' },
    };

    const modifierDefinitions = {
      shift: { keysym: 0xffe1, code: 'ShiftLeft' },
      control: { keysym: 0xffe3, code: 'ControlLeft' },
      alt: { keysym: 0xffe9, code: 'AltLeft' },
      meta: { keysym: 0xffeb, code: 'MetaLeft' },
    };

    const canSendKeyboard = () => Boolean(rfb && !viewOnly && typeof rfb.sendKey === 'function');

    const sendKeyEvent = (definition, down) => {
      if (!definition || !canSendKeyboard()) {
        return;
      }

      rfb.sendKey(definition.keysym, definition.code, down);
    };

    const pressKey = (definition) => {
      sendKeyEvent(definition, true);
      sendKeyEvent(definition, false);
    };

    const keysymForCharacter = (character) => {
      const codePoint = character.codePointAt(0);
      if (!codePoint) {
        return null;
      }

      if (codePoint >= 0x20 && codePoint <= 0xff) {
        return codePoint;
      }

      return 0x01000000 | codePoint;
    };

    const sendCharacter = (character) => {
      if (character === '\\n' || character === '\\r') {
        pressKey(keyDefinitions.enter);
        return;
      }
      if (character === '\\t') {
        pressKey(keyDefinitions.tab);
        return;
      }

      const keysym = keysymForCharacter(character);
      if (keysym) {
        pressKey({ keysym, code: 'Unidentified' });
      }
    };

    const sendText = (text) => {
      for (const character of Array.from(text || '')) {
        sendCharacter(character);
      }
    };

    const keyDefinitionForLetter = (letter) => ({
      keysym: keysymForCharacter(letter.toLowerCase()),
      code: 'Key' + letter.toUpperCase(),
    });

    const shortcutDefinitions = {
      'ctrl-a': { modifiers: ['control'], key: keyDefinitionForLetter('a') },
      'ctrl-c': { modifiers: ['control'], key: keyDefinitionForLetter('c') },
      'ctrl-v': { modifiers: ['control'], key: keyDefinitionForLetter('v') },
      'ctrl-x': { modifiers: ['control'], key: keyDefinitionForLetter('x') },
      'ctrl-z': { modifiers: ['control'], key: keyDefinitionForLetter('z') },
      'ctrl-alt-del': { modifiers: ['control', 'alt'], key: keyDefinitions.delete },
    };

    const updateModifierButtons = () => {
      updateNativeState({ pressedModifiers: Array.from(pressedModifiers) });
    };

    const releaseModifiers = () => {
      for (const modifier of Array.from(pressedModifiers)) {
        const definition = modifierDefinitions[modifier];
        if (definition) {
          sendKeyEvent(definition, false);
        }
        pressedModifiers.delete(modifier);
      }
      updateModifierButtons();
    };

    const toggleModifier = (modifier) => {
      const definition = modifierDefinitions[modifier];
      if (!definition || !canSendKeyboard()) {
        return;
      }

      if (pressedModifiers.has(modifier)) {
        sendKeyEvent(definition, false);
        pressedModifiers.delete(modifier);
      } else {
        sendKeyEvent(definition, true);
        pressedModifiers.add(modifier);
      }
      updateModifierButtons();
    };

    const updateKeyboardControls = () => {
      updateNativeState({ keyboardActive });
    };

    const setKeyboardActive = (active) => {
      const nextActive = Boolean(active && !viewOnly);
      if (keyboardActive === nextActive) {
        updateKeyboardControls();
        return;
      }

      keyboardActive = nextActive;
      if (!keyboardActive) {
        releaseModifiers();
      }
      updateKeyboardControls();
    };

    const pressSpecialKey = (name) => {
      pressKey(keyDefinitions[name]);
    };

    const sendShortcut = (shortcut) => {
      if (!canSendKeyboard()) {
        return;
      }

      const shortcutDefinition = shortcutDefinitions[shortcut];
      if (!shortcutDefinition) {
        return;
      }

      releaseModifiers();
      const modifierKeys = shortcutDefinition.modifiers
        .map((modifier) => modifierDefinitions[modifier])
        .filter(Boolean);
      for (const modifier of modifierKeys) {
        sendKeyEvent(modifier, true);
      }
      pressKey(shortcutDefinition.key);
      for (const modifier of modifierKeys.slice().reverse()) {
        sendKeyEvent(modifier, false);
      }
    };

    const requestFullFramebufferUpdate = () => {
      if (!rfb || !rfb._sock || !rfb._fbWidth || !rfb._fbHeight) {
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

    const canvasFor = (currentRfb) => currentRfb && currentRfb._canvas ? currentRfb._canvas : null;

    const clearLongPressTimer = () => {
      if (longPressTimer) {
        window.clearTimeout(longPressTimer);
        longPressTimer = null;
      }
    };

    const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

    const pointFromClient = (clientX, clientY) => {
      const canvas = canvasFor(rfb);
      if (!canvas) {
        return null;
      }

      const bounds = canvas.getBoundingClientRect();
      const maxX = Math.max(0, bounds.width - 1);
      const maxY = Math.max(0, bounds.height - 1);
      return {
        x: clamp(clientX - bounds.left, 0, maxX),
        y: clamp(clientY - bounds.top, 0, maxY),
      };
    };

    const centeredPointer = () => {
      const canvas = canvasFor(rfb);
      if (!canvas) {
        return null;
      }

      const bounds = canvas.getBoundingClientRect();
      return {
        x: Math.max(0, bounds.width / 2),
        y: Math.max(0, bounds.height / 2),
      };
    };

    const clampTouchPointer = () => {
      const canvas = canvasFor(rfb);
      if (!canvas || !touchPointer) {
        return null;
      }

      const bounds = canvas.getBoundingClientRect();
      touchPointer = {
        x: clamp(touchPointer.x, 0, Math.max(0, bounds.width - 1)),
        y: clamp(touchPointer.y, 0, Math.max(0, bounds.height - 1)),
      };
      return touchPointer;
    };

    const updateTouchCursor = () => {
      const canvas = canvasFor(rfb);
      if (
        !touchCursor ||
        !desktopStage ||
        !canvas ||
        viewOnly ||
        pointerMode !== 'touchpad' ||
        !touchPointer
      ) {
        if (touchCursor) {
          touchCursor.hidden = true;
        }
        return;
      }

      const pointer = clampTouchPointer();
      if (!pointer) {
        touchCursor.hidden = true;
        return;
      }

      const canvasBounds = canvas.getBoundingClientRect();
      const stageBounds = desktopStage.getBoundingClientRect();
      touchCursor.hidden = false;
      touchCursor.style.transform =
        'translate(' +
        Math.round(canvasBounds.left - stageBounds.left + pointer.x) +
        'px, ' +
        Math.round(canvasBounds.top - stageBounds.top + pointer.y) +
        'px)';
    };

    const currentPointerMask = () => (dragLocked ? activeMouseButton : 0);

    const sendPointer = (mask) => {
      if (!rfb || viewOnly) {
        return;
      }

      if (!touchPointer) {
        touchPointer = centeredPointer();
      }

      const pointer = clampTouchPointer();
      if (!pointer || typeof rfb._sendMouse !== 'function') {
        return;
      }

      rfb._mouseButtonMask = mask;
      rfb._mousePos = { x: pointer.x, y: pointer.y };
      rfb._sendMouse(pointer.x, pointer.y, mask);
      updateTouchCursor();
    };

    const schedulePointerMove = (mask = currentPointerMask()) => {
      pendingPointerMask = mask;
      if (pendingPointerFrame) {
        return;
      }

      pendingPointerFrame = window.requestAnimationFrame(() => {
        pendingPointerFrame = null;
        sendPointer(pendingPointerMask);
      });
    };

    const clickPointer = (buttonMask = activeMouseButton) => {
      if (dragLocked) {
        return;
      }

      sendPointer(buttonMask);
      window.setTimeout(() => sendPointer(0), 48);
    };

    const sendWheelStep = (mask) => {
      sendPointer(mask);
      sendPointer(currentPointerMask());
    };

    const setActiveMouseButton = (buttonMask, switchToTouchpad = true) => {
      if (switchToTouchpad && pointerMode !== 'touchpad') {
        setPointerMode('touchpad');
      }
      activeMouseButton = buttonMask;
      if (dragLocked) {
        sendPointer(activeMouseButton);
      }
      updateNativeState({ activeMouseButton });
    };

    const setDragLocked = (locked) => {
      dragLocked = Boolean(locked);
      sendPointer(currentPointerMask());
      updateNativeState({ dragLocked });
    };

    const updatePointerMode = () => {
      if (rfb) {
        rfb.dragViewport = false;
        rfb.showDotCursor = pointerMode === 'direct';
      }

      if (pointerMode !== 'touchpad') {
        clearLongPressTimer();
        touchpadState = null;
        if (dragLocked) {
          setDragLocked(false);
        }
      } else if (!touchPointer) {
        touchPointer = centeredPointer();
      }
      updateTouchCursor();
      updateNativeState({ pointerMode });
    };

    const setPointerMode = (nextMode) => {
      pointerMode = nextMode === 'touchpad' ? 'touchpad' : 'direct';
      updatePointerMode();
    };

    const touchCenter = (touches) => ({
      x: (touches[0].clientX + touches[1].clientX) / 2,
      y: (touches[0].clientY + touches[1].clientY) / 2,
    });

    const touchDistance = (touches) =>
      Math.hypot(
        touches[0].clientX - touches[1].clientX,
        touches[0].clientY - touches[1].clientY,
      );

    const startMultiTouch = (touches) => {
      clearLongPressTimer();
      const center = touchCenter(touches);
      const distance = touchDistance(touches);
      if (!touchPointer && !viewOnly) {
        touchPointer = pointFromClient(center.x, center.y) || centeredPointer();
      }
      touchpadState = {
        type: 'multi',
        lastX: center.x,
        lastY: center.y,
        startDistance: distance,
        startZoom: zoomLevel,
        wheelX: 0,
        wheelY: 0,
        zooming: false,
      };
      updateTouchCursor();
    };

    const startSingleTouch = (touch) => {
      clearLongPressTimer();
      if (!touchPointer) {
        touchPointer = pointFromClient(touch.clientX, touch.clientY) || centeredPointer();
      }
      touchpadState = {
        type: 'move',
        lastX: touch.clientX,
        lastY: touch.clientY,
        startX: touch.clientX,
        startY: touch.clientY,
        moved: false,
        longPressed: false,
      };
      updateTouchCursor();
      if (!dragLocked) {
        longPressTimer = window.setTimeout(() => {
          if (touchpadState && touchpadState.type === 'move' && !touchpadState.moved) {
            touchpadState.longPressed = true;
            clickPointer(0x4);
          }
        }, longPressDelay);
      }
    };

    const handleTouchpadStart = (event) => {
      const touches = Array.from(event.touches);
      if (touches.length >= 2) {
        startMultiTouch(touches);
        return;
      }

      if (touches.length === 1 && !viewOnly && pointerMode === 'touchpad') {
        startSingleTouch(touches[0]);
      }
    };

    const handleMultiTouchMove = (touches) => {
      const center = touchCenter(touches);
      const distance = touchDistance(touches);
      if (!touchpadState || touchpadState.type !== 'multi') {
        startMultiTouch(touches);
        return;
      }

      const dx = center.x - touchpadState.lastX;
      const dy = center.y - touchpadState.lastY;
      const distanceDelta = Math.abs(distance - touchpadState.startDistance);
      const shouldZoom = touchpadState.zooming || distanceDelta > pinchZoomThreshold;

      if (shouldZoom) {
        touchpadState.zooming = true;
        setZoomLevel(
          touchpadState.startZoom * (distance / Math.max(1, touchpadState.startDistance)),
          center.x,
          center.y,
          false,
        );
        panViewportBy(-dx, -dy);
      } else if (zoomLevel > 1 || canPanViewport()) {
        panViewportBy(-dx, -dy);
      } else {
        touchpadState.wheelX += dx;
        touchpadState.wheelY += dy;

        while (touchpadState.wheelY > touchWheelStep) {
          sendWheelStep(0x8);
          touchpadState.wheelY -= touchWheelStep;
        }
        while (touchpadState.wheelY < -touchWheelStep) {
          sendWheelStep(0x10);
          touchpadState.wheelY += touchWheelStep;
        }
        while (touchpadState.wheelX > touchWheelStep) {
          sendWheelStep(0x20);
          touchpadState.wheelX -= touchWheelStep;
        }
        while (touchpadState.wheelX < -touchWheelStep) {
          sendWheelStep(0x40);
          touchpadState.wheelX += touchWheelStep;
        }
      }

      touchpadState.lastX = center.x;
      touchpadState.lastY = center.y;
    };

    const handleTouchpadMove = (event) => {
      const touches = Array.from(event.touches);
      if (touches.length === 0) {
        return;
      }

      if (touches.length >= 2) {
        handleMultiTouchMove(touches);
        return;
      }

      if (!touchpadState || touchpadState.type !== 'move' || viewOnly || pointerMode !== 'touchpad') {
        return;
      }

      const touch = touches[0];
      const dx = touch.clientX - touchpadState.lastX;
      const dy = touch.clientY - touchpadState.lastY;
      const totalDx = touch.clientX - touchpadState.startX;
      const totalDy = touch.clientY - touchpadState.startY;
      touchpadState.lastX = touch.clientX;
      touchpadState.lastY = touch.clientY;
      if (Math.hypot(totalDx, totalDy) > tapMoveThreshold) {
        touchpadState.moved = true;
        clearLongPressTimer();
      }

      if (!touchPointer) {
        touchPointer = pointFromClient(touch.clientX, touch.clientY) || centeredPointer();
      }
      if (touchPointer) {
        touchPointer = {
          x: touchPointer.x + dx * touchpadSpeed,
          y: touchPointer.y + dy * touchpadSpeed,
        };
        clampTouchPointer();
        schedulePointerMove();
      }
    };

    const handleTouchpadEnd = (event) => {
      clearLongPressTimer();
      if (!touchpadState) {
        return;
      }

      const wasMultiTouch = touchpadState.type === 'multi';
      if (
        event.touches.length === 0 &&
        touchpadState.type === 'move' &&
        !touchpadState.moved &&
        !touchpadState.longPressed
      ) {
        clickPointer();
      }

      if (event.touches.length === 0) {
        touchpadState = null;
        if (!wasMultiTouch && !dragLocked) {
          sendPointer(0);
        }
        return;
      }

      const touches = Array.from(event.touches);
      if (touches.length >= 2) {
        startMultiTouch(touches);
        return;
      }

      if (!viewOnly && pointerMode === 'touchpad') {
        startSingleTouch(touches[0]);
        if (touchpadState) {
          touchpadState.moved = true;
          touchpadState.longPressed = true;
        }
        return;
      }

      touchpadState = null;
    };

    const handleTouchpadTouch = (event) => {
      const touches = Array.from(event.touches);
      const handlingMultiTouch = touches.length >= 2 || (touchpadState && touchpadState.type === 'multi');
      const handlingTrackpad = !viewOnly && pointerMode === 'touchpad';
      if (!handlingMultiTouch && !handlingTrackpad) {
        return;
      }

      event.preventDefault();
      event.stopImmediatePropagation();

      if (event.type === 'touchstart') {
        handleTouchpadStart(event);
      } else if (event.type === 'touchmove') {
        handleTouchpadMove(event);
      } else {
        handleTouchpadEnd(event);
      }
    };

    for (const type of ['touchstart', 'touchmove', 'touchend', 'touchcancel']) {
      target.addEventListener(type, handleTouchpadTouch, {
        capture: true,
        passive: false,
      });
    }

    const displayFor = (currentRfb) => currentRfb && currentRfb._display ? currentRfb._display : null;

    const fitScaleFor = (screen, width, height) => {
      if (!screen.width || !screen.height || !width || !height) {
        return 1;
      }

      const fitScale = Math.min(width / screen.width, height / screen.height);
      return Number.isFinite(fitScale) && fitScale > 0 ? fitScale : 1;
    };

    const expectedScaleFor = (screen, width, height) => {
      const baseScale = autoScale ? fitScaleFor(screen, width, height) : 1;
      return Math.max(0.05, baseScale * zoomLevel);
    };

    const expectedViewportSizeFor = (screen, width, height, scale) => {
      if (autoScale && zoomLevel === 1) {
        return {
          width: screen.width,
          height: screen.height,
        };
      }

      return {
        width: Math.min(screen.width, Math.max(1, Math.floor(width / scale))),
        height: Math.min(screen.height, Math.max(1, Math.floor(height / scale))),
      };
    };

    const viewportOriginFor = (screen, viewportWidth, viewportHeight, x, y) => {
      const maxX = Math.max(screen.x, screen.x + screen.width - viewportWidth);
      const maxY = Math.max(screen.y, screen.y + screen.height - viewportHeight);
      return {
        x: clamp(x, screen.x, maxX),
        y: clamp(y, screen.y, maxY),
      };
    };

    const viewportCenterFor = (display, screen) => {
      const viewport = display._viewportLoc || {};
      const centerX = Number(viewport.x) + Number(viewport.w) / 2;
      const centerY = Number(viewport.y) + Number(viewport.h) / 2;
      const withinScreen =
        Number.isFinite(centerX) &&
        Number.isFinite(centerY) &&
        centerX >= screen.x &&
        centerX <= screen.x + screen.width &&
        centerY >= screen.y &&
        centerY <= screen.y + screen.height;

      if (withinScreen) {
        return { x: centerX, y: centerY };
      }

      return {
        x: screen.x + screen.width / 2,
        y: screen.y + screen.height / 2,
      };
    };

    const viewportAnchorFromClient = (clientX, clientY) => {
      const display = displayFor(rfb);
      const canvas = canvasFor(rfb);
      if (!display || !canvas) {
        return null;
      }

      const bounds = canvas.getBoundingClientRect();
      const localX = clamp(clientX - bounds.left, 0, Math.max(0, bounds.width - 1));
      const localY = clamp(clientY - bounds.top, 0, Math.max(0, bounds.height - 1));
      return {
        localX,
        localY,
        remoteX: display.absX(localX),
        remoteY: display.absY(localY),
      };
    };

    const centerClientPoint = () => {
      const bounds = target.getBoundingClientRect();
      return {
        x: bounds.left + bounds.width / 2,
        y: bounds.top + bounds.height / 2,
      };
    };

    const layoutMatches = (display, screen, width, height) => {
      const expectedScale = expectedScaleFor(screen, width, height);
      const viewportSize = expectedViewportSizeFor(screen, width, height, expectedScale);
      const actualScale = Number(display.scale || 0);
      const viewport = display._viewportLoc || {};
      const viewportX = Number(viewport.x || 0);
      const viewportY = Number(viewport.y || 0);
      const viewportW = Number(viewport.w || 0);
      const viewportH = Number(viewport.h || 0);
      const viewportSizeMatches =
        Math.abs(viewportW - viewportSize.width) <= 1 &&
        Math.abs(viewportH - viewportSize.height) <= 1;
      const viewportInBounds =
        viewportX >= screen.x - 1 &&
        viewportY >= screen.y - 1 &&
        viewportX + viewportW <= screen.x + screen.width + 1 &&
        viewportY + viewportH <= screen.y + screen.height + 1;

      return (
        Math.abs(actualScale - expectedScale) < 0.002 &&
        Boolean(display.clipViewport) &&
        viewportSizeMatches &&
        viewportInBounds
      );
    };

    const displaySize = (currentRfb) => {
      const display = displayFor(currentRfb);
      const width = Number((display && display.width) || (currentRfb && currentRfb._fbWidth) || 0);
      const height = Number((display && display.height) || (currentRfb && currentRfb._fbHeight) || 0);
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
          const label = String((screen && screen.label) || index + 1);
          return {
            id: String((screen && screen.id) || 'display-' + (index + 1)),
            label,
            title: String((screen && screen.title) || 'Screen ' + label),
            x: normalizedInteger(screen && screen.x),
            y: normalizedInteger(screen && screen.y),
            width: normalizedInteger(screen && screen.width),
            height: normalizedInteger(screen && screen.height),
            primary: Boolean(screen && screen.primary),
          };
        })
        .filter((screen) => screen.width > 0 && screen.height > 0);
    };

    const configuredScreens = normalizeScreenLayout(configuredScreenLayout);

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
          id: 'screen-' + (index + 1),
          label: String(index + 1),
          title: 'Screen ' + (index + 1),
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
          id: 'screen-' + (index + 1),
          label: String(index + 1),
          title: 'Screen ' + (index + 1),
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
      const size = displaySize(rfb);
      if (!size.width || !size.height) {
        return [];
      }

      const allScreens = {
        id: 'all',
        label: 'All',
        title: 'All screens',
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
      };
      let screens = configuredScreensFor(size.width, size.height);
      if (screens.length < 2) {
        screens = detectedScreens(size.width, size.height);
      }
      if (screens.length < 2) {
        return [allScreens];
      }

      return [allScreens, ...screens];
    };

    const preferredNativeScreenId = (options) => {
      if (options.length < 2) {
        return 'all';
      }

      const screens = options.filter((screen) => screen.id !== 'all');
      return (screens.find((screen) => screen.primary) || screens[0] || options[0] || {}).id || 'all';
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
      updateNativeState({
        screens: screenOptions,
        selectedScreenId,
      });
    };

    const selectedScreen = () =>
      screenOptions.find((screen) => screen.id === selectedScreenId) || screenOptions[0] || null;

    const canPanViewport = () => {
      const display = displayFor(rfb);
      const screen = selectedScreen();
      const viewport = display && display._viewportLoc ? display._viewportLoc : null;
      return Boolean(
        display &&
        screen &&
        viewport &&
        (Number(viewport.w || 0) < screen.width || Number(viewport.h || 0) < screen.height)
      );
    };

    const panViewportBy = (deltaCssX, deltaCssY) => {
      const display = displayFor(rfb);
      if (!display || !canPanViewport()) {
        return false;
      }

      const scale = Number(display.scale || 1) || 1;
      const viewport = display._viewportLoc || {};
      const beforeX = Number(viewport.x || 0);
      const beforeY = Number(viewport.y || 0);
      display.viewportChangePos(deltaCssX / scale, deltaCssY / scale);
      const nextViewport = display._viewportLoc || {};
      const moved =
        Math.abs(Number(nextViewport.x || 0) - beforeX) > 0.5 ||
        Math.abs(Number(nextViewport.y || 0) - beforeY) > 0.5;
      if (moved) {
        lastAppliedViewport = '';
        updateTouchCursor();
      }
      return moved;
    };

    const applySelectedScreen = (force = false, anchor = null) => {
      const display = displayFor(rfb);
      const screen = selectedScreen();
      if (!display || !screen) {
        return;
      }

      const bounds = target.getBoundingClientRect();
      const width = Math.max(1, bounds.width);
      const height = Math.max(1, bounds.height);
      const displayWidth = Number(display.width || 0);
      const displayHeight = Number(display.height || 0);
      const expectedScale = expectedScaleFor(screen, width, height);
      const viewportSize = expectedViewportSizeFor(screen, width, height, expectedScale);
      const viewportCenter = viewportCenterFor(display, screen);
      const preferredOrigin = anchor
        ? {
            x: anchor.remoteX - anchor.localX / expectedScale,
            y: anchor.remoteY - anchor.localY / expectedScale,
          }
        : {
            x: viewportCenter.x - viewportSize.width / 2,
            y: viewportCenter.y - viewportSize.height / 2,
          };
      const viewportOrigin = viewportOriginFor(
        screen,
        viewportSize.width,
        viewportSize.height,
        preferredOrigin.x,
        preferredOrigin.y,
      );
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
        Math.round(zoomLevel * 1000),
        autoScale ? 'fit' : 'native',
      ].join(':');

      if (!anchor && !force && layoutKey === lastAppliedViewport && layoutMatches(display, screen, width, height)) {
        return;
      }
      lastAppliedViewport = layoutKey;

      if (rfb && rfb._screen) {
        rfb._screen.style.overflow = 'hidden';
      }
      applyViewerBackground(rfb);

      rfb.scaleViewport = false;
      rfb.clipViewport = true;
      display.clipViewport = true;
      display.viewportChangeSize(viewportSize.width, viewportSize.height);
      const viewport = display._viewportLoc || { x: 0, y: 0 };
      display.viewportChangePos(viewportOrigin.x - viewport.x, viewportOrigin.y - viewport.y);
      display.scale = expectedScale;
      if (force) {
        scheduleFullFramebufferRefresh();
      }
      updateTouchCursor();
    };

    const setZoomLevel = (nextZoom, clientX, clientY, scheduleRetries = true) => {
      const nextLevel = clamp(nextZoom, minZoom, maxZoom);
      const anchor =
        Number.isFinite(clientX) && Number.isFinite(clientY)
          ? viewportAnchorFromClient(clientX, clientY)
          : null;
      if (Math.abs(nextLevel - zoomLevel) < 0.002) {
        return;
      }

      zoomLevel = nextLevel;
      lastAppliedViewport = '';
      updateZoomControls();
      updateScaleButton();
      applySelectedScreen(true, anchor);
      if (scheduleRetries) {
        scheduleLayoutRetry();
      }
    };

    const zoomBy = (factor) => {
      const center = centerClientPoint();
      setZoomLevel(zoomLevel * factor, center.x, center.y);
    };

    const refreshScreenOptions = () => {
      const nextOptions = buildScreenOptions();
      const selectedExists = nextOptions.some((screen) => screen.id === selectedScreenId);
      let selectedChanged = false;
      if (!selectedExists) {
        selectedScreenId = preferredNativeScreenId(nextOptions);
        hasUserSelectedScreen = false;
        selectedChanged = true;
      } else if (!hasUserSelectedScreen && nextOptions.length > 1) {
        const preferredScreenId = preferredNativeScreenId(nextOptions);
        if (selectedScreenId !== preferredScreenId) {
          selectedScreenId = preferredScreenId;
          selectedChanged = true;
        }
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
      if (resizeObserverFrame) {
        window.cancelAnimationFrame(resizeObserverFrame);
        resizeObserverFrame = null;
      }
      screenOptions = [];
      selectedScreenId = 'all';
      hasUserSelectedScreen = false;
      lastAppliedViewport = '';
      renderScreenSwitcher();
    };

    const startScreenRefresh = () => {
      stopScreenRefresh();
      if (typeof ResizeObserver !== 'undefined') {
        resizeObserver = new ResizeObserver(() => {
          if (resizeObserverFrame) {
            return;
          }

          resizeObserverFrame = window.requestAnimationFrame(() => {
            resizeObserverFrame = null;
            refreshScreenOptions();
          });
        });
        resizeObserver.observe(target);
      }
      screenRefreshTimer = window.setInterval(refreshScreenOptions, 1000);
      window.setTimeout(refreshScreenOptions, 50);
      window.setTimeout(refreshScreenOptions, 500);
      scheduleLayoutRetry();
    };

    const toggleScaleMode = () => {
      if (zoomLevel > 1 || !autoScale) {
        zoomLevel = 1;
        autoScale = true;
      } else {
        autoScale = false;
      }
      lastAppliedViewport = '';
      updateZoomControls();
      updateScaleButton();
      scheduleLayoutRetry();
    };

    const selectScreenById = (screenId) => {
      if (!screenOptions.some((screen) => screen.id === screenId)) {
        return;
      }

      hasUserSelectedScreen = true;
      selectedScreenId = screenId;
      lastAppliedViewport = '';
      renderScreenSwitcher();
      applySelectedScreen(true);
      scheduleLayoutRetry();
    };

    const submitCredentials = (credentials) => {
      if (!rfb) {
        connect();
        return;
      }

      rfb.sendCredentials(credentials || {});
      hideCredentials();
      setStatus('Authenticating');
    };

    const handleNativeCommand = (command) => {
      if (!command || typeof command !== 'object') {
        return;
      }

      const type = command.type || command.action;
      if (type === 'requestState') {
        flushNativeState();
      } else if (type === 'toggleScale') {
        toggleScaleMode();
      } else if (type === 'zoomIn') {
        zoomBy(zoomStep);
      } else if (type === 'zoomOut') {
        zoomBy(1 / zoomStep);
      } else if (type === 'setZoom') {
        const center = centerClientPoint();
        setZoomLevel(Number(command.zoomLevel), center.x, center.y);
      } else if (type === 'selectScreen') {
        selectScreenById(String(command.screenId || 'all'));
      } else if (type === 'setPointerMode') {
        setPointerMode(command.mode);
      } else if (type === 'setMouseButton') {
        const mask = Number(command.buttonMask);
        setActiveMouseButton([0x1, 0x2, 0x4].includes(mask) ? mask : 0x1);
      } else if (type === 'toggleDragLock') {
        if (pointerMode !== 'touchpad') {
          setPointerMode('touchpad');
        }
        setDragLocked(!dragLocked);
      } else if (type === 'setDragLock') {
        if (pointerMode !== 'touchpad') {
          setPointerMode('touchpad');
        }
        setDragLocked(Boolean(command.locked));
      } else if (type === 'pressKey') {
        pressSpecialKey(command.key);
      } else if (type === 'toggleModifier') {
        toggleModifier(command.modifier);
      } else if (type === 'releaseModifiers') {
        releaseModifiers();
      } else if (type === 'shortcut') {
        sendShortcut(command.shortcut);
      } else if (type === 'sendText') {
        sendText(String(command.text || ''));
      } else if (type === 'sendCredentials') {
        submitCredentials(command.credentials);
      } else if (type === 'reconnect') {
        window.latitudeReconnect(Boolean(command.force));
      } else if (type === 'refresh') {
        scheduleFullFramebufferRefresh();
      }
    };

    window.latitudeMobileCommand = handleNativeCommand;

    const configureRfb = (nextRfb) => {
      applyViewerBackground(nextRfb);
      nextRfb.viewOnly = viewOnly;
      nextRfb.scaleViewport = false;
      nextRfb.resizeSession = false;
      nextRfb.clipViewport = false;
      nextRfb.dragViewport = false;
      nextRfb.focusOnClick = !viewOnly;
      nextRfb.showDotCursor = pointerMode === 'direct';
      nextRfb.qualityLevel = 6;
      nextRfb.compressionLevel = 2;
      updatePointerMode();

      nextRfb.addEventListener('connect', () => {
        clearReconnectTimer();
        reconnectDelay = 1000;
        hideCredentials();
        updateNativeState({ connected: true });
        applyViewerBackground(nextRfb);
        setStatus('Connected');
        startScreenRefresh();
        scheduleFullFramebufferRefresh();
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

        setKeyboardActive(false, false);
        rfb = null;
        stopScreenRefresh();
        touchPointer = null;
        updateNativeState({ connected: false });
        updateTouchCursor();
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

    };

    const connect = () => {
      if (rfb) {
        return;
      }

      clearReconnectTimer();
      hideCredentials();
      target.replaceChildren();
      touchPointer = null;
      updateTouchCursor();
      setStatus('Connecting');
      try {
        const nextRfb = new RFB(target, websocketUrl, { shared: true });
        rfb = nextRfb;
        applyViewerBackground(nextRfb);
        configureRfb(nextRfb);
      } catch (error) {
        setStatus(error.message || 'Connection failed', true);
        scheduleReconnect();
      }
    };

    window.latitudeReconnect = (force) => {
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

    window.addEventListener('focus', () => window.latitudeReconnect(false));
    window.addEventListener('online', () => window.latitudeReconnect(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        window.latitudeReconnect(false);
      }
    });
    window.addEventListener('beforeunload', () => {
      reconnectEnabled = false;
      setKeyboardActive(false, false);
      clearReconnectTimer();
      stopScreenRefresh();
      if (rfb) {
        rfb.disconnect();
      }
    });

    updateScaleButton();
    updateZoomControls();
    updateKeyboardControls();
    setActiveMouseButton(activeMouseButton, false);
    updatePointerMode();
    applyViewerBackground();
    updateNativeState({ ready: true });
    flushNativeState();

    connect();
  </script>
</body>
</html>`;
}
