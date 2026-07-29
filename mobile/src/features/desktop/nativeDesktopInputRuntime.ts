export const nativeDesktopInputRuntime = String.raw`
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

    const handleTouchStart = (event) => {
      if (viewOnly || !controlGranted) return;
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
      if (viewOnly || !controlGranted || !touchState) return;
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
      sendPointerMove();
    };

    const handleTouchEnd = (event) => {
      if (viewOnly || !controlGranted || !touchState) return;
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
