export const desktopInputRuntime = String.raw`
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
      if (viewOnly || !controlGranted || !definition) return;
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
      updateViewerState({ pressedModifiers: Array.from(pressedModifiers) });
    };

    const releaseModifiers = () => {
      for (const modifier of Array.from(pressedModifiers)) {
        sendKey(modifierDefinitions[modifier], false);
      }
      pressedModifiers.clear();
      updateModifiers();
    };

    const releaseAllInput = () => {
      const shouldRelease = !viewOnly && controlGranted;
      pressedModifiers.clear();
      dragLocked = false;
      if (shouldRelease) {
        send({ type: 'release_input' });
      }
      updateViewerState({
        dragLocked,
        pressedModifiers: [],
      });
    };

    const toggleModifier = (modifier) => {
      const definition = modifierDefinitions[modifier];
      if (!definition || viewOnly || !controlGranted) return;
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
      if (!definition || viewOnly || !controlGranted) return;
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
      const center = touchCenter(touches);
      touchState = {
        type: 'multi',
        lastX: center.x,
        lastY: center.y,
        startDistance: touchDistance(touches),
        startZoom: zoomLevel,
        wheelX: 0,
        wheelY: 0,
        zooming: false,
      };
    };

    const startSingleTouch = (touch, suppressTap = false) => {
      const point = touchPoint(touch);
      if (!point) return;
      if (pointerMode === 'direct') {
        pointerX = point.x;
        pointerY = point.y;
        sendPointer(dragLocked ? activeMouseButton : 0);
      }
      touchState = {
        type: 'single',
        startX: touch.clientX,
        startY: touch.clientY,
        lastX: touch.clientX,
        lastY: touch.clientY,
        moved: suppressTap,
      };
    };

    const handleTouchStart = (event) => {
      const touches = Array.from(event.touches);
      const handlingMultiTouch = touches.length >= 2;
      const handlingPointer = !viewOnly && controlGranted;
      if (!handlingMultiTouch && !handlingPointer) return;
      event.preventDefault();
      if (handlingMultiTouch) {
        startMultiTouch(touches);
      } else if (touches[0]) {
        startSingleTouch(touches[0]);
      }
    };

    const handleTouchMove = (event) => {
      if (!touchState) return;
      event.preventDefault();
      const touches = Array.from(event.touches);

      if (touches.length >= 2) {
        if (touchState.type !== 'multi') {
          startMultiTouch(touches);
          return;
        }
        const center = touchCenter(touches);
        const distance = touchDistance(touches);
        const dx = center.x - touchState.lastX;
        const dy = center.y - touchState.lastY;
        const distanceDelta = Math.abs(distance - touchState.startDistance);
        const shouldZoom = touchState.zooming || distanceDelta > pinchZoomThreshold;

        if (shouldZoom) {
          touchState.zooming = true;
          setZoomLevelAt(
            touchState.startZoom * (distance / Math.max(1, touchState.startDistance)),
            center.x,
            center.y,
          );
          viewportPanX += dx;
          viewportPanY += dy;
          applyViewportTransform();
          updateTouchCursor();
        } else if (canPanViewport()) {
          viewportPanX += dx;
          viewportPanY += dy;
          applyViewportTransform();
          updateTouchCursor();
        } else if (!viewOnly && controlGranted) {
          touchState.wheelX += dx;
          touchState.wheelY += dy;
          if (Math.abs(touchState.wheelY) >= wheelStep || Math.abs(touchState.wheelX) >= wheelStep) {
            send({
              type: 'wheel',
              delta_x: touchState.wheelX >= wheelStep ? 120 : touchState.wheelX <= -wheelStep ? -120 : 0,
              delta_y: touchState.wheelY >= wheelStep ? -120 : touchState.wheelY <= -wheelStep ? 120 : 0,
            });
            touchState.wheelX = 0;
            touchState.wheelY = 0;
          }
        }
        touchState.lastX = center.x;
        touchState.lastY = center.y;
        return;
      }

      if (
        touchState.type !== 'single' ||
        viewOnly ||
        !controlGranted ||
        !touches[0]
      ) return;
      const touch = touches[0];
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
      sendPointerMove();
    };

    const handleTouchEnd = (event) => {
      if (!touchState) return;
      event.preventDefault();
      const touches = Array.from(event.touches);
      const wasMultiTouch = touchState.type === 'multi';
      const wasTap =
        touchState.type === 'single' &&
        !touchState.moved &&
        event.type !== 'touchcancel' &&
        touches.length === 0;

      if (touches.length >= 2) {
        startMultiTouch(touches);
      } else if (touches.length === 1 && !viewOnly && controlGranted) {
        startSingleTouch(touches[0], wasMultiTouch);
      } else {
        touchState = null;
      }

      if (wasTap) clickPointer();
      else if (!wasMultiTouch && touches.length === 0 && !dragLocked) sendPointer(0);
    };

    for (const type of ['touchstart', 'touchmove', 'touchend', 'touchcancel']) {
      stage.addEventListener(
        type,
        type === 'touchstart'
          ? handleTouchStart
          : type === 'touchmove'
            ? handleTouchMove
            : handleTouchEnd,
        { passive: false },
      );
    }

    const handleDesktopCommand = (command) => {
      if (!command || typeof command !== 'object') return;
      const type = command.type || command.action;
      if (type === 'requestState') {
        flushViewerState();
      } else if (type === 'toggleScale') {
        if (zoomLevel > 1 || !autoScale) {
          zoomLevel = 1;
          autoScale = true;
          viewportPanX = 0;
          viewportPanY = 0;
        } else {
          autoScale = false;
        }
        updateViewerState({ autoScale, zoomLevel });
        layoutCanvas();
      } else if (type === 'zoomIn' || type === 'zoomOut') {
        const factor = type === 'zoomIn' ? zoomStep : 1 / zoomStep;
        const bounds = stage.getBoundingClientRect();
        setZoomLevelAt(
          zoomLevel * factor,
          bounds.left + bounds.width / 2,
          bounds.top + bounds.height / 2,
        );
      } else if (type === 'selectScreen') {
        selectedScreenId = String(command.screenId || 'all');
        viewportPanX = 0;
        viewportPanY = 0;
        const screen = selectedScreen();
        if (screen) {
          pointerX = screen.width / 2;
          pointerY = screen.height / 2;
        }
        updateViewerState({ selectedScreenId });
        renderFrame();
      } else if (type === 'setPointerMode' && pointerModeValues.has(command.mode)) {
        pointerMode = command.mode;
        updateViewerState({ pointerMode });
        updateTouchCursor();
      } else if (type === 'setMouseButton') {
        const mask = Number(command.buttonMask);
        activeMouseButton = [0x1, 0x2, 0x4].includes(mask) ? mask : 0x1;
        if (dragLocked) sendPointer(activeMouseButton);
        updateViewerState({ activeMouseButton });
      } else if (type === 'toggleDragLock') {
        dragLocked = !dragLocked;
        sendPointer(dragLocked ? activeMouseButton : 0);
        updateViewerState({ dragLocked });
      } else if (type === 'pressKey') {
        pressKey(keyDefinitions[command.key]);
      } else if (type === 'toggleModifier') {
        toggleModifier(command.modifier);
      } else if (type === 'releaseModifiers') {
        releaseModifiers();
      } else if (type === 'releaseInput') {
        releaseAllInput();
      } else if (type === 'shortcut') {
        sendShortcut(command.shortcut);
      } else if (type === 'sendText') {
        if (controlGranted) {
          send({ type: 'text', text: String(command.text || '') });
        }
      } else if (type === 'refresh') {
        send({ type: 'refresh' });
      } else if (type === 'reconnect') {
        window.latitudeReconnect(Boolean(command.force));
      }
    };
`;
