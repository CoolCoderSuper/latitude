import type { DesktopScreen } from '../../types';

export function desktopDocument(
  label: string,
  websocketUrl: string,
  viewOnly: boolean,
  screenLayout: DesktopScreen[] = [],
): string {
  const labelJson = JSON.stringify(label);
  const websocketUrlJson = JSON.stringify(websocketUrl);
  const viewOnlyJson = JSON.stringify(viewOnly);
  const screenLayoutJson = JSON.stringify(screenLayout);

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
      background: #050505;
      color: #edf4f1;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    body {
      box-sizing: border-box;
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding: env(safe-area-inset-top, 0) 8px env(safe-area-inset-bottom, 0);
      overscroll-behavior: none;
      user-select: none;
      -webkit-touch-callout: none;
      -webkit-user-select: none;
    }

    .bar {
      box-sizing: border-box;
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 8px;
      min-height: 34px;
      border-bottom: 1px solid #2e3936;
      padding: 6px 2px;
      color: #aeb9c7;
      font-size: 12px;
      font-weight: 800;
    }

    .mode {
      border: 1px solid #367064;
      border-radius: 8px;
      padding: 4px 7px;
      color: #bbf7d0;
      background: rgba(18, 52, 33, 0.82);
      font-size: 11px;
      font-weight: 900;
      white-space: nowrap;
    }

    .screens {
      display: inline-flex;
      flex: 0 0 auto;
      align-items: center;
      overflow: hidden;
      border: 1px solid #33413d;
      border-radius: 8px;
      background: #101514;
    }

    .screens[hidden] {
      display: none;
    }

    .screens button {
      min-width: 30px;
      min-height: 28px;
      border: 0;
      border-right: 1px solid #33413d;
      padding: 0 8px;
      color: #aeb9c7;
      background: transparent;
      font: inherit;
      font-size: 11px;
      font-weight: 900;
    }

    .screens button:last-child {
      border-right: 0;
    }

    .screens button.active {
      color: #061413;
      background: #2aa79c;
    }

    .control {
      flex: 0 0 auto;
      min-width: 34px;
      min-height: 28px;
      border: 1px solid #33413d;
      border-radius: 8px;
      padding: 0 8px;
      color: #aeb9c7;
      background: #101514;
      font: inherit;
      font-size: 11px;
      font-weight: 900;
    }

    .control.active {
      border-color: #2aa79c;
      color: #061413;
      background: #2aa79c;
    }

    .control[hidden] {
      display: none;
    }

    .control:disabled {
      color: #52615e;
      opacity: 0.62;
    }

    .zoom-controls {
      display: inline-flex;
      flex: 0 0 auto;
      align-items: center;
      overflow: hidden;
      border: 1px solid #33413d;
      border-radius: 8px;
      background: #101514;
    }

    .zoom-controls .control {
      min-width: 30px;
      border: 0;
      border-radius: 0;
      background: transparent;
    }

    .zoom-level {
      min-width: 40px;
      border-right: 1px solid #33413d;
      border-left: 1px solid #33413d;
      color: #aeb9c7;
      font-size: 11px;
      font-weight: 900;
      line-height: 28px;
      text-align: center;
      white-space: nowrap;
    }

    .status {
      margin-left: auto;
      max-width: 126px;
      overflow: hidden;
      color: #8fe0ad;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .status.error {
      color: #ffb3a7;
    }

    .pointer-tools {
      display: flex;
      align-items: center;
      gap: 8px;
      min-height: 36px;
      overflow-x: auto;
      padding: 0 2px 2px;
      scrollbar-width: none;
      -webkit-overflow-scrolling: touch;
    }

    .pointer-tools::-webkit-scrollbar {
      display: none;
    }

    .pointer-tools[hidden] {
      display: none;
    }

    .keyboard-panel {
      position: fixed;
      right: 12px;
      bottom: calc(12px + env(safe-area-inset-bottom, 0px));
      left: 12px;
      z-index: 5;
      display: grid;
      max-height: min(430px, calc(100vh - 96px));
      gap: 10px;
      overflow-y: auto;
      border: 1px solid #33413d;
      border-radius: 8px;
      padding: 10px;
      background: rgba(16, 21, 20, 0.98);
      box-shadow: 0 18px 52px rgba(0, 0, 0, 0.46);
      box-sizing: border-box;
      -webkit-overflow-scrolling: touch;
    }

    .keyboard-panel[hidden] {
      display: none;
    }

    .keyboard-panel-head {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .keyboard-panel-title {
      flex: 1 1 auto;
      color: #edf4f1;
      font-weight: 900;
    }

    .keyboard-tools {
      display: grid;
      gap: 8px;
    }

    .keyboard-group {
      display: flex;
      flex-wrap: wrap;
      align-items: stretch;
      overflow: hidden;
      border: 1px solid #33413d;
      border-radius: 8px;
      background: #101514;
    }

    .keyboard-group .control {
      flex: 1 0 auto;
      min-width: 52px;
      border: 0;
      border-right: 1px solid #33413d;
      border-radius: 0;
      background: transparent;
    }

    .keyboard-group .control:last-child {
      border-right: 0;
    }

    .keyboard-input {
      width: 100%;
      min-height: 84px;
      max-height: 150px;
      border: 1px solid #33413d;
      border-radius: 8px;
      padding: 9px 10px;
      color: #edf4f1;
      background: #050505;
      caret-color: #2aa79c;
      box-sizing: border-box;
      font: inherit;
      outline: 0;
      pointer-events: auto;
      resize: vertical;
    }

    .keyboard-input:focus {
      border-color: #2aa79c;
      box-shadow: 0 0 0 2px rgba(42, 167, 156, 0.2);
    }

    .keyboard-actions {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .keyboard-actions .keyboard-send {
      flex: 1 1 auto;
      min-width: 90px;
    }

    .keyboard-actions .keyboard-clear {
      flex: 0 0 auto;
    }

    @media (min-width: 700px) {
      .keyboard-panel {
        right: auto;
        left: 50%;
        width: min(520px, calc(100vw - 24px));
        transform: translateX(-50%);
      }
    }

    .pointer-group {
      display: inline-flex;
      flex: 0 0 auto;
      align-items: center;
      overflow: hidden;
      border: 1px solid #33413d;
      border-radius: 8px;
      background: #101514;
    }

    .pointer-group .control {
      border: 0;
      border-right: 1px solid #33413d;
      border-radius: 0;
      background: transparent;
    }

    .pointer-group .control:last-child {
      border-right: 0;
    }

    .pointer-group .control.active {
      color: #061413;
      background: #2aa79c;
    }

    .mouse-button {
      min-width: 38px;
    }

    .drag-lock {
      min-width: 52px;
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
      border: 1px solid #2e3936;
      border-radius: 8px;
      background: #050505;
      box-sizing: border-box;
      touch-action: none;
      user-select: none;
      -webkit-user-select: none;
    }

    #desktop canvas {
      touch-action: none;
      user-select: none;
      -webkit-user-select: none;
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

    .credentials {
      position: absolute;
      top: 48px;
      left: 16px;
      z-index: 3;
      display: grid;
      gap: 8px;
      width: min(320px, calc(100% - 32px));
      border: 1px solid #33413d;
      border-radius: 8px;
      padding: 12px;
      background: rgba(16, 21, 20, 0.96);
      box-shadow: 0 16px 40px rgba(0, 0, 0, 0.35);
    }

    .credentials[hidden],
    .credentials label[hidden] {
      display: none;
    }

    .credentials label {
      display: grid;
      gap: 5px;
      color: #aeb9c7;
      font-size: 12px;
      font-weight: 900;
    }

    .credentials input {
      box-sizing: border-box;
      width: 100%;
      min-height: 38px;
      border: 1px solid #33413d;
      border-radius: 8px;
      padding: 0 10px;
      color: #edf4f1;
      background: #101514;
      font: inherit;
    }

    .credentials button {
      min-height: 38px;
      border: 1px solid #2aa79c;
      border-radius: 8px;
      color: #061413;
      background: #2aa79c;
      font: inherit;
      font-weight: 900;
    }
  </style>
</head>
<body>
  <div class="bar">
    <span class="mode">${viewOnly ? 'View only' : 'Control'}</span>
    <span class="screens" hidden aria-label="Screens"></span>
    <button class="control scale" type="button" aria-pressed="true">Fit</button>
    <span class="zoom-controls" role="group" aria-label="Zoom">
      <button class="control zoom-out" type="button" aria-label="Zoom out">-</button>
      <span class="zoom-level" aria-live="polite">100%</span>
      <button class="control zoom-in" type="button" aria-label="Zoom in">+</button>
    </span>
    <button class="control keyboard-toggle" type="button" aria-pressed="false">Keys</button>
    <button class="control fullscreen" type="button" aria-pressed="false">Full</button>
    <span class="status">Connecting</span>
  </div>
  <div class="pointer-tools" hidden>
    <span class="pointer-group" role="group" aria-label="Pointer mode">
      <button class="control pointer-mode touchpad active" type="button" data-pointer-mode="touchpad" aria-pressed="true">Pad</button>
      <button class="control pointer-mode direct" type="button" data-pointer-mode="direct" aria-pressed="false">Direct</button>
    </span>
    <span class="pointer-group" role="group" aria-label="Mouse buttons">
      <button class="control mouse-button left active" type="button" data-mouse-button="1" aria-pressed="true">L</button>
      <button class="control mouse-button middle" type="button" data-mouse-button="2" aria-pressed="false">M</button>
      <button class="control mouse-button right" type="button" data-mouse-button="4" aria-pressed="false">R</button>
      <button class="control drag-lock" type="button" aria-pressed="false">Drag</button>
    </span>
  </div>
  <form class="credentials" hidden>
    <label class="credential-user" hidden>
      Username
      <input type="text" autocomplete="username" />
    </label>
    <label class="credential-password">
      Password
      <input type="password" autocomplete="current-password" />
    </label>
    <label class="credential-target" hidden>
      Target
      <input type="text" />
    </label>
    <button type="submit">Connect</button>
  </form>
  <div class="desktop-stage">
    <div id="desktop" tabindex="0"></div>
    <div class="keyboard-panel" hidden role="dialog" aria-label="Keyboard">
      <div class="keyboard-panel-head">
        <strong class="keyboard-panel-title">Keys</strong>
        <button class="control keyboard-close" type="button" aria-label="Close keyboard">Close</button>
      </div>
      <textarea class="keyboard-input" rows="3" autocomplete="off" autocorrect="off" autocapitalize="none" spellcheck="false" inputmode="text" aria-label="Text to send" placeholder="Text to send"></textarea>
      <div class="keyboard-actions">
        <button class="control keyboard-send active" type="button">Send</button>
        <button class="control keyboard-clear" type="button">Clear</button>
      </div>
      <div class="keyboard-tools">
        <span class="keyboard-group" role="group" aria-label="Modifier keys">
          <button class="control" type="button" data-modifier-key="control" aria-pressed="false">Ctrl</button>
          <button class="control" type="button" data-modifier-key="alt" aria-pressed="false">Alt</button>
          <button class="control" type="button" data-modifier-key="shift" aria-pressed="false">Shift</button>
          <button class="control" type="button" data-modifier-key="meta" aria-pressed="false">Win</button>
        </span>
        <span class="keyboard-group" role="group" aria-label="Special keys">
          <button class="control" type="button" data-key-name="escape">Esc</button>
          <button class="control" type="button" data-key-name="tab">Tab</button>
          <button class="control" type="button" data-key-name="enter">Enter</button>
          <button class="control" type="button" data-key-name="backspace">Bksp</button>
          <button class="control" type="button" data-key-name="delete">Del</button>
        </span>
        <span class="keyboard-group" role="group" aria-label="Navigation keys">
          <button class="control" type="button" data-key-name="home">Home</button>
          <button class="control" type="button" data-key-name="end">End</button>
          <button class="control" type="button" data-key-name="pageup">PgUp</button>
          <button class="control" type="button" data-key-name="pagedown">PgDn</button>
        </span>
        <span class="keyboard-group" role="group" aria-label="Arrow keys">
          <button class="control" type="button" data-key-name="left">Left</button>
          <button class="control" type="button" data-key-name="up">Up</button>
          <button class="control" type="button" data-key-name="down">Down</button>
          <button class="control" type="button" data-key-name="right">Right</button>
        </span>
        <span class="keyboard-group" role="group" aria-label="Command shortcuts">
          <button class="control" type="button" data-key-combo="ctrl-a">All</button>
          <button class="control" type="button" data-key-combo="ctrl-c">Copy</button>
          <button class="control" type="button" data-key-combo="ctrl-v">Paste</button>
          <button class="control" type="button" data-key-combo="ctrl-x">Cut</button>
          <button class="control" type="button" data-key-combo="ctrl-z">Undo</button>
          <button class="control" type="button" data-key-combo="ctrl-alt-del">CAD</button>
        </span>
      </div>
    </div>
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
      var status = document.querySelector('.status');
      if (status && !window.latitudeViewerStarted) {
        status.textContent = event.message || 'Viewer script failed';
        status.classList.add('error');
      }
    });
    window.setTimeout(function () {
      var status = document.querySelector('.status');
      if (status && !window.latitudeViewerStarted) {
        status.textContent = 'Viewer script failed';
        status.classList.add('error');
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
    const target = document.getElementById('desktop');
    const statusElement = document.querySelector('.status');
    const screensElement = document.querySelector('.screens');
    const scaleButton = document.querySelector('.scale');
    const zoomOutButton = document.querySelector('.zoom-out');
    const zoomInButton = document.querySelector('.zoom-in');
    const zoomLevelElement = document.querySelector('.zoom-level');
    const keyboardToggle = document.querySelector('.keyboard-toggle');
    const keyboardPanel = document.querySelector('.keyboard-panel');
    const keyboardInput = document.querySelector('.keyboard-input');
    const keyboardCloseButton = document.querySelector('.keyboard-close');
    const keyboardSendButton = document.querySelector('.keyboard-send');
    const keyboardClearButton = document.querySelector('.keyboard-clear');
    const keyboardKeyButtons = Array.from(document.querySelectorAll('[data-key-name]'));
    const keyboardModifierButtons = Array.from(document.querySelectorAll('[data-modifier-key]'));
    const keyboardComboButtons = Array.from(document.querySelectorAll('[data-key-combo]'));
    const fullscreenButton = document.querySelector('.fullscreen');
    const pointerTools = document.querySelector('.pointer-tools');
    const pointerModeButtons = Array.from(document.querySelectorAll('[data-pointer-mode]'));
    const mouseButtonElements = Array.from(document.querySelectorAll('[data-mouse-button]'));
    const dragLockButton = document.querySelector('.drag-lock');
    const touchCursor = document.querySelector('.touch-cursor');
    const desktopStage = document.querySelector('.desktop-stage');
    const credentialsElement = document.querySelector('.credentials');
    const credentialUser = document.querySelector('.credential-user');
    const credentialPassword = document.querySelector('.credential-password');
    const credentialTarget = document.querySelector('.credential-target');

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

    const setStatus = (text, isError = false) => {
      statusElement.textContent = text;
      statusElement.classList.toggle('error', Boolean(isError));
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
      credentialsElement.hidden = true;
    };

    const showCredentials = (types) => {
      const required = new Set(types || ['password']);
      credentialUser.hidden = !required.has('username');
      credentialPassword.hidden = !required.has('password');
      credentialTarget.hidden = !required.has('target');
      credentialsElement.hidden = false;
      const firstInput = credentialsElement.querySelector('label:not([hidden]) input');
      if (firstInput) {
        firstInput.focus();
      }
    };

    const collectCredentials = () => {
      const payload = {};
      const username = credentialUser.querySelector('input').value;
      const password = credentialPassword.querySelector('input').value;
      const targetValue = credentialTarget.querySelector('input').value;
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
      scaleButton.textContent = autoScale ? 'Fit' : '1:1';
      scaleButton.classList.toggle('active', autoScale && zoomLevel === 1);
      scaleButton.setAttribute('aria-pressed', String(autoScale && zoomLevel === 1));
      scaleButton.title =
        zoomLevel > 1 || !autoScale ? 'Reset to fitted view' : 'Switch to 1:1 view';
    };

    const updateZoomControls = () => {
      const roundedZoom = Math.round(zoomLevel * 100);
      zoomLevelElement.textContent = roundedZoom + '%';
      zoomOutButton.disabled = zoomLevel <= minZoom + 0.001;
      zoomInButton.disabled = zoomLevel >= maxZoom - 0.001;
      zoomOutButton.title = 'Zoom out';
      zoomInButton.title = 'Zoom in';
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
      for (const button of keyboardModifierButtons) {
        const active = pressedModifiers.has(button.dataset.modifierKey);
        button.classList.toggle('active', active);
        button.setAttribute('aria-pressed', String(active));
      }
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
      keyboardToggle.hidden = viewOnly;
      keyboardPanel.hidden = viewOnly || !keyboardActive;
      keyboardToggle.classList.toggle('active', keyboardActive);
      keyboardToggle.setAttribute('aria-pressed', String(keyboardActive));
      keyboardInput.disabled = viewOnly || !keyboardActive;
    };

    const resetKeyboardInput = () => {
      keyboardInput.value = '';
    };

    const focusKeyboardInput = () => {
      if (viewOnly) {
        return;
      }

      keyboardInput.focus({ preventScroll: true });
      window.setTimeout(() => {
        keyboardInput.focus({ preventScroll: true });
      }, 40);
    };

    const setKeyboardActive = (active, focusInput = true) => {
      const nextActive = Boolean(active && !viewOnly);
      if (keyboardActive === nextActive) {
        if (nextActive && focusInput) {
          focusKeyboardInput();
        }
        updateKeyboardControls();
        return;
      }

      keyboardActive = nextActive;
      if (!keyboardActive) {
        releaseModifiers();
        keyboardInput.blur();
      }
      updateKeyboardControls();
      if (keyboardActive && focusInput) {
        focusKeyboardInput();
      }
    };

    const pressSpecialKey = (name) => {
      pressKey(keyDefinitions[name]);
    };

    const sendKeyboardInputText = () => {
      const value = keyboardInput.value;
      if (!value) {
        focusKeyboardInput();
        return;
      }
      if (!canSendKeyboard()) {
        return;
      }

      sendText(value);
      resetKeyboardInput();
      focusKeyboardInput();
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

    const updateFullscreenButton = () => {
      const fullscreenHost = document.documentElement;
      if (!document.fullscreenEnabled || !fullscreenHost.requestFullscreen) {
        fullscreenButton.hidden = true;
        return;
      }

      const isFullscreen = document.fullscreenElement === fullscreenHost;
      fullscreenButton.textContent = isFullscreen ? 'Exit' : 'Full';
      fullscreenButton.classList.toggle('active', isFullscreen);
      fullscreenButton.setAttribute('aria-pressed', String(isFullscreen));
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
      for (const button of mouseButtonElements) {
        const active = Number(button.dataset.mouseButton) === activeMouseButton;
        button.classList.toggle('active', active);
        button.setAttribute('aria-pressed', String(active));
      }
      if (dragLocked) {
        sendPointer(activeMouseButton);
      }
    };

    const setDragLocked = (locked) => {
      dragLocked = Boolean(locked);
      dragLockButton.classList.toggle('active', dragLocked);
      dragLockButton.setAttribute('aria-pressed', String(dragLocked));
      sendPointer(currentPointerMask());
    };

    const updatePointerMode = () => {
      pointerTools.hidden = viewOnly;
      for (const button of pointerModeButtons) {
        const active = button.dataset.pointerMode === pointerMode;
        button.classList.toggle('active', active);
        button.setAttribute('aria-pressed', String(active));
      }

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
      screensElement.hidden = screenOptions.length < 2;
      screensElement.replaceChildren();
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
          applySelectedScreen(true);
          scheduleLayoutRetry();
        });
        screensElement.appendChild(button);
      }
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
      if (!selectedExists) {
        selectedScreenId = 'all';
      }

      if (!sameScreenOptions(screenOptions, nextOptions)) {
        screenOptions = nextOptions;
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

    scaleButton.addEventListener('click', () => {
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
    });

    zoomOutButton.addEventListener('click', () => zoomBy(1 / zoomStep));
    zoomInButton.addEventListener('click', () => zoomBy(zoomStep));

    keyboardToggle.addEventListener('click', () => {
      setKeyboardActive(!keyboardActive, false);
    });

    keyboardCloseButton.addEventListener('click', () => {
      setKeyboardActive(false, false);
    });

    keyboardSendButton.addEventListener('click', () => {
      sendKeyboardInputText();
    });

    keyboardClearButton.addEventListener('click', () => {
      resetKeyboardInput();
      focusKeyboardInput();
    });

    keyboardInput.addEventListener('focus', () => {
      keyboardActive = !viewOnly;
      updateKeyboardControls();
    });

    keyboardInput.addEventListener('keydown', (event) => {
      if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
        event.preventDefault();
        sendKeyboardInputText();
        return;
      }

      if (event.key === 'Escape') {
        event.preventDefault();
        setKeyboardActive(false, false);
      }
    });

    for (const button of keyboardKeyButtons) {
      button.addEventListener('click', () => pressSpecialKey(button.dataset.keyName));
    }

    for (const button of keyboardModifierButtons) {
      button.addEventListener('click', () => {
        toggleModifier(button.dataset.modifierKey);
      });
    }

    for (const button of keyboardComboButtons) {
      button.addEventListener('click', () => {
        sendShortcut(button.dataset.keyCombo);
      });
    }

    for (const button of pointerModeButtons) {
      button.addEventListener('click', () => setPointerMode(button.dataset.pointerMode));
    }

    for (const button of mouseButtonElements) {
      button.addEventListener('click', () => setActiveMouseButton(Number(button.dataset.mouseButton)));
    }

    dragLockButton.addEventListener('click', () => {
      if (pointerMode !== 'touchpad') {
        setPointerMode('touchpad');
      }
      setDragLocked(!dragLocked);
    });

    fullscreenButton.addEventListener('click', async () => {
      const fullscreenHost = document.documentElement;
      try {
        if (document.fullscreenElement === fullscreenHost) {
          await document.exitFullscreen();
        } else {
          await fullscreenHost.requestFullscreen();
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
      nextRfb.showDotCursor = pointerMode === 'direct';
      nextRfb.qualityLevel = 6;
      nextRfb.compressionLevel = 2;
      updatePointerMode();

      nextRfb.addEventListener('connect', () => {
        clearReconnectTimer();
        reconnectDelay = 1000;
        hideCredentials();
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

    credentialsElement.addEventListener('submit', (event) => {
      event.preventDefault();
      if (!rfb) {
        connect();
        return;
      }
      rfb.sendCredentials(collectCredentials());
      hideCredentials();
      setStatus('Authenticating');
    });

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
    updateFullscreenButton();
    updateKeyboardControls();
    setActiveMouseButton(activeMouseButton, false);
    updatePointerMode();

    connect();
  </script>
</body>
</html>`;
}
