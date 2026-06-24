import {
  CircleDot,
  Keyboard,
  Lock,
  Monitor,
  MousePointer2,
  MousePointerClick,
  Move,
  RefreshCw,
  Send,
  Touchpad,
  X,
  ZoomIn,
  ZoomOut,
} from 'lucide-react-native';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import {
  AppState,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
  type StyleProp,
  type TextStyle,
} from 'react-native';
import WebView from 'react-native-webview';

import { normalizeBaseUrl } from '../../api';
import { useTheme, type ThemeColors, type ThemeMode } from '../../theme';
import type { DesktopScreen, RootDesktopLink, SessionRecord } from '../../types';
import { desktopDocument } from './desktopDocument';

type PointerMode = 'touchpad' | 'direct';

type DesktopViewerScreen = {
  id: string;
  label: string;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  primary: boolean;
};

type DesktopViewerState = {
  ready: boolean;
  connected: boolean;
  status: string;
  statusIsError: boolean;
  viewOnly: boolean;
  autoScale: boolean;
  zoomLevel: number;
  selectedScreenId: string;
  screens: DesktopViewerScreen[];
  pointerMode: PointerMode;
  activeMouseButton: number;
  dragLocked: boolean;
  pressedModifiers: string[];
  credentialsRequired: string[] | null;
};

type DesktopCommand = Record<string, unknown> & {
  type: string;
};

type CredentialValues = {
  username: string;
  password: string;
  target: string;
};

const MOUSE_BUTTONS = [
  { label: 'L', mask: 0x1, title: 'Left click' },
  { label: 'M', mask: 0x2, title: 'Middle click' },
  { label: 'R', mask: 0x4, title: 'Right click' },
];

const MODIFIER_KEYS = [
  { key: 'control', label: 'Ctrl' },
  { key: 'alt', label: 'Alt' },
  { key: 'shift', label: 'Shift' },
  { key: 'meta', label: 'Win' },
];

const SPECIAL_KEYS = [
  { key: 'escape', label: 'Esc' },
  { key: 'tab', label: 'Tab' },
  { key: 'enter', label: 'Enter' },
  { key: 'backspace', label: 'Bksp' },
  { key: 'delete', label: 'Del' },
];

const NAVIGATION_KEYS = [
  { key: 'home', label: 'Home' },
  { key: 'end', label: 'End' },
  { key: 'pageup', label: 'PgUp' },
  { key: 'pagedown', label: 'PgDn' },
];

const ARROW_KEYS = [
  { key: 'left', label: 'Left' },
  { key: 'up', label: 'Up' },
  { key: 'down', label: 'Down' },
  { key: 'right', label: 'Right' },
];

const SHORTCUT_KEYS = [
  { shortcut: 'ctrl-a', label: 'All' },
  { shortcut: 'ctrl-c', label: 'Copy' },
  { shortcut: 'ctrl-v', label: 'Paste' },
  { shortcut: 'ctrl-x', label: 'Cut' },
  { shortcut: 'ctrl-z', label: 'Undo' },
  { shortcut: 'ctrl-alt-del', label: 'CAD' },
];

type DesktopChrome = {
  surface: string;
  panel: string;
  input: string;
  text: string;
  muted: string;
  accent: string;
  onAccent: string;
  success: string;
  danger: string;
  dangerBg: string;
  border: string;
  modalBackdrop: string;
  viewerBackground: string;
};

export function RootDesktopPanel({
  rootDesktop,
  session,
}: {
  rootDesktop: RootDesktopLink;
  session: SessionRecord;
}) {
  const { colors, mode, styles } = useTheme();
  const chrome = useMemo(() => createDesktopChrome(colors, mode), [colors, mode]);
  const controlStyles = useMemo(
    () => createDesktopControlStyles(chrome),
    [chrome],
  );
  const webViewRef = useRef<WebView>(null);
  const [viewerState, setViewerState] = useState<DesktopViewerState>(() =>
    initialViewerState(rootDesktop.view_only, rootDesktop.screens ?? []),
  );
  const [keyboardOpen, setKeyboardOpen] = useState(false);
  const [keyboardText, setKeyboardText] = useState('');
  const [credentialValues, setCredentialValues] = useState<CredentialValues>({
    username: '',
    password: '',
    target: '',
  });
  const desktopUrl = useMemo(
    () => desktopWebSocketUrl(session.baseUrl, rootDesktop.href, session.token),
    [rootDesktop.href, session.baseUrl, session.token],
  );
  const desktopHtml = useMemo(
    () =>
      desktopDocument(
        rootDesktop.label,
        desktopUrl,
        rootDesktop.view_only,
        rootDesktop.screens ?? [],
        chrome.viewerBackground,
      ),
    [
      chrome.viewerBackground,
      desktopUrl,
      rootDesktop.label,
      rootDesktop.screens,
      rootDesktop.view_only,
    ],
  );
  const controlsDisabled = !viewerState.ready;
  const canControl = !viewerState.viewOnly;
  const credentialFields = viewerState.credentialsRequired ?? [];

  const sendCommand = useCallback((command: DesktopCommand) => {
    const payload = JSON.stringify(command);
    webViewRef.current?.injectJavaScript(
      `window.latitudeMobileCommand && window.latitudeMobileCommand(${payload}); true;`,
    );
  }, []);

  const requestState = useCallback(() => {
    sendCommand({ type: 'requestState' });
  }, [sendCommand]);

  useEffect(() => {
    setViewerState(initialViewerState(rootDesktop.view_only, rootDesktop.screens ?? []));
    setKeyboardOpen(false);
    setKeyboardText('');
    setCredentialValues({ username: '', password: '', target: '' });
  }, [rootDesktop.screens, rootDesktop.view_only]);

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        sendCommand({ type: 'reconnect', force: true });
      }
    });
    return () => subscription.remove();
  }, [sendCommand]);

  const handleMessage = useCallback((event: { nativeEvent: { data: string } }) => {
    const message = parseBridgeMessage(event.nativeEvent.data);
    if (message?.type !== 'desktop-state') {
      return;
    }

    setViewerState((current) => mergeViewerState(current, message.state));
  }, []);

  const sendKeyboardText = useCallback(() => {
    if (!keyboardText || !canControl) {
      return;
    }

    sendCommand({ type: 'sendText', text: keyboardText });
    setKeyboardText('');
  }, [canControl, keyboardText, sendCommand]);

  const sendCredentials = useCallback(() => {
    sendCommand({
      type: 'sendCredentials',
      credentials: credentialValues,
    });
    setCredentialValues({ username: '', password: '', target: '' });
  }, [credentialValues, sendCommand]);

  return (
    <View
      style={[
        styles.desktopPanel,
        { backgroundColor: chrome.viewerBackground },
      ]}
    >
      <WebView
        ref={webViewRef}
        bounces={false}
        domStorageEnabled
        javaScriptEnabled
        keyboardDisplayRequiresUserAction={false}
        mixedContentMode="always"
        onLoadEnd={requestState}
        onMessage={handleMessage}
        originWhitelist={['*']}
        scrollEnabled={false}
        setSupportMultipleWindows={false}
        showsHorizontalScrollIndicator={false}
        showsVerticalScrollIndicator={false}
        source={{ html: desktopHtml, baseUrl: normalizeBaseUrl(session.baseUrl) }}
        startInLoadingState
        style={[styles.webView, { backgroundColor: chrome.viewerBackground }]}
      />
      <View pointerEvents="box-none" style={controlStyles.overlay}>
        <ScrollView
          horizontal
          keyboardShouldPersistTaps="handled"
          showsHorizontalScrollIndicator={false}
          style={controlStyles.railScroll}
          contentContainerStyle={controlStyles.topBar}
        >
          <View
            style={[
              controlStyles.statusPill,
              viewerState.statusIsError && controlStyles.statusPillError,
            ]}
          >
            <CircleDot
              color={
                viewerState.statusIsError
                  ? chrome.danger
                  : chrome.success
              }
              size={14}
            />
            <Text
              numberOfLines={1}
              style={[
                controlStyles.statusText,
                viewerState.statusIsError && controlStyles.statusTextError,
              ]}
            >
              {viewerState.status || (viewerState.connected ? 'Connected' : 'Desktop')}
            </Text>
          </View>
          {viewerState.screens.length > 1 && (
            <View style={controlStyles.screenList}>
              {viewerState.screens.map((screen) => (
                <ControlButton
                  active={screen.id === viewerState.selectedScreenId}
                  controlStyles={controlStyles}
                  disabled={controlsDisabled}
                  key={screen.id}
                  label={screen.label}
                  onPress={() =>
                    sendCommand({ type: 'selectScreen', screenId: screen.id })
                  }
                  textStyle={controlStyles.screenButtonText}
                  title={screen.title}
                />
              ))}
            </View>
          )}
          <ControlButton
            active={viewerState.autoScale && viewerState.zoomLevel <= 1.01}
            controlStyles={controlStyles}
            disabled={controlsDisabled}
            icon={<Monitor color={buttonColor(chrome, viewerState.autoScale)} size={16} />}
            label={viewerState.autoScale && viewerState.zoomLevel <= 1.01 ? 'Fit' : '1:1'}
            onPress={() => sendCommand({ type: 'toggleScale' })}
          />
          <View style={controlStyles.zoomGroup}>
            <ControlButton
              controlStyles={controlStyles}
              disabled={controlsDisabled || viewerState.zoomLevel <= 1.01}
              icon={<ZoomOut color={buttonColor(chrome)} size={16} />}
              label=""
              onPress={() => sendCommand({ type: 'zoomOut' })}
              title="Zoom out"
            />
            <Text numberOfLines={1} style={controlStyles.zoomText}>
              {Math.round(viewerState.zoomLevel * 100)}%
            </Text>
            <ControlButton
              controlStyles={controlStyles}
              disabled={controlsDisabled || viewerState.zoomLevel >= 2.99}
              icon={<ZoomIn color={buttonColor(chrome)} size={16} />}
              label=""
              onPress={() => sendCommand({ type: 'zoomIn' })}
              title="Zoom in"
            />
          </View>
        </ScrollView>

        {credentialFields.length > 0 && (
          <CredentialPrompt
            chrome={chrome}
            controlStyles={controlStyles}
            fields={credentialFields}
            onChange={setCredentialValues}
            onSubmit={sendCredentials}
            values={credentialValues}
          />
        )}

        {keyboardOpen && canControl && (
          <KeyboardAvoidingView
            behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
            pointerEvents="box-none"
            style={controlStyles.keyboardHost}
          >
            <View style={controlStyles.keyboardPanel}>
              <View style={controlStyles.panelHeader}>
                <Text style={controlStyles.panelTitle}>Keys</Text>
                <ControlButton
                  controlStyles={controlStyles}
                  icon={<X color={chrome.text} size={16} />}
                  label=""
                  onPress={() => setKeyboardOpen(false)}
                  title="Close keyboard controls"
                />
              </View>
              <View style={controlStyles.sendRow}>
                <TextInput
                  autoCapitalize="none"
                  autoCorrect={false}
                  multiline
                  onChangeText={setKeyboardText}
                  placeholder="Text to send"
                  placeholderTextColor={chrome.muted}
                  spellCheck={false}
                  style={controlStyles.keyboardInput}
                  value={keyboardText}
                />
                <ControlButton
                  active
                  controlStyles={controlStyles}
                  disabled={!keyboardText}
                  icon={<Send color={chrome.onAccent} size={16} />}
                  label="Send"
                  onPress={sendKeyboardText}
                />
              </View>
              <ScrollView
                keyboardShouldPersistTaps="handled"
                showsVerticalScrollIndicator={false}
                style={controlStyles.keyboardTools}
                contentContainerStyle={controlStyles.keyboardToolsContent}
              >
                <KeyRow
                  controlStyles={controlStyles}
                  items={MODIFIER_KEYS.map((item) => ({
                    active: viewerState.pressedModifiers.includes(item.key),
                    label: item.label,
                    onPress: () =>
                      sendCommand({ type: 'toggleModifier', modifier: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={SPECIAL_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () => sendCommand({ type: 'pressKey', key: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={NAVIGATION_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () => sendCommand({ type: 'pressKey', key: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={ARROW_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () => sendCommand({ type: 'pressKey', key: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={SHORTCUT_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () =>
                      sendCommand({ type: 'shortcut', shortcut: item.shortcut }),
                  }))}
                />
              </ScrollView>
            </View>
          </KeyboardAvoidingView>
        )}

        <ScrollView
          horizontal
          keyboardShouldPersistTaps="handled"
          showsHorizontalScrollIndicator={false}
          style={controlStyles.railScroll}
          contentContainerStyle={controlStyles.bottomBar}
        >
          {canControl && (
            <>
              <View style={controlStyles.segment}>
                <ControlButton
                  active={viewerState.pointerMode === 'touchpad'}
                  controlStyles={controlStyles}
                  disabled={controlsDisabled}
                  icon={
                    <Touchpad
                      color={buttonColor(chrome, viewerState.pointerMode === 'touchpad')}
                      size={16}
                    />
                  }
                  label="Pad"
                  onPress={() =>
                    sendCommand({ type: 'setPointerMode', mode: 'touchpad' })
                  }
                />
                <ControlButton
                  active={viewerState.pointerMode === 'direct'}
                  controlStyles={controlStyles}
                  disabled={controlsDisabled}
                  icon={
                    <MousePointer2
                      color={buttonColor(chrome, viewerState.pointerMode === 'direct')}
                      size={16}
                    />
                  }
                  label="Direct"
                  onPress={() =>
                    sendCommand({ type: 'setPointerMode', mode: 'direct' })
                  }
                />
              </View>
              <View style={controlStyles.segment}>
                {MOUSE_BUTTONS.map((button) => (
                  <ControlButton
                    active={viewerState.activeMouseButton === button.mask}
                    controlStyles={controlStyles}
                    disabled={controlsDisabled}
                    key={button.mask}
                    label={button.label}
                    onPress={() =>
                      sendCommand({
                        type: 'setMouseButton',
                        buttonMask: button.mask,
                      })
                    }
                    title={button.title}
                  />
                ))}
                <ControlButton
                  active={viewerState.dragLocked}
                  controlStyles={controlStyles}
                  disabled={controlsDisabled}
                  icon={
                    viewerState.dragLocked ? (
                      <Lock color={buttonColor(chrome, true)} size={16} />
                    ) : (
                      <Move color={buttonColor(chrome)} size={16} />
                    )
                  }
                  label="Drag"
                  onPress={() => sendCommand({ type: 'toggleDragLock' })}
                />
              </View>
              <ControlButton
                active={keyboardOpen}
                controlStyles={controlStyles}
                disabled={controlsDisabled}
                icon={<Keyboard color={buttonColor(chrome, keyboardOpen)} size={17} />}
                label="Keys"
                onPress={() => setKeyboardOpen((open) => !open)}
              />
            </>
          )}
          <ControlButton
            controlStyles={controlStyles}
            disabled={controlsDisabled}
            icon={<MousePointerClick color={chrome.text} size={17} />}
            label=""
            onPress={() => sendCommand({ type: 'refresh' })}
            title="Refresh desktop"
          />
          <ControlButton
            controlStyles={controlStyles}
            icon={<RefreshCw color={chrome.text} size={17} />}
            label=""
            onPress={() => sendCommand({ type: 'reconnect', force: true })}
            title="Reconnect"
          />
        </ScrollView>
      </View>
    </View>
  );
}

function ControlButton({
  active = false,
  controlStyles,
  disabled = false,
  icon,
  label,
  onPress,
  textStyle,
  title,
}: {
  active?: boolean;
  controlStyles: DesktopControlStyles;
  disabled?: boolean;
  icon?: ReactNode;
  label: string;
  onPress: () => void;
  textStyle?: StyleProp<TextStyle>;
  title?: string;
}) {
  return (
    <Pressable
      accessibilityLabel={title || label}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        controlStyles.controlButton,
        active && controlStyles.controlButtonActive,
        disabled && controlStyles.controlButtonDisabled,
        pressed && !disabled && controlStyles.controlButtonPressed,
      ]}
    >
      {icon}
      {label ? (
        <Text
          numberOfLines={1}
          style={[
            controlStyles.controlButtonText,
            active && controlStyles.controlButtonTextActive,
            disabled && controlStyles.controlButtonTextDisabled,
            textStyle,
          ]}
        >
          {label}
        </Text>
      ) : null}
    </Pressable>
  );
}

function KeyRow({
  controlStyles,
  items,
}: {
  controlStyles: DesktopControlStyles;
  items: Array<{
    active?: boolean;
    label: string;
    onPress: () => void;
  }>;
}) {
  return (
    <View style={controlStyles.keyRow}>
      {items.map((item) => (
        <ControlButton
          active={item.active}
          controlStyles={controlStyles}
          key={item.label}
          label={item.label}
          onPress={item.onPress}
        />
      ))}
    </View>
  );
}

function CredentialPrompt({
  chrome,
  controlStyles,
  fields,
  onChange,
  onSubmit,
  values,
}: {
  chrome: DesktopChrome;
  controlStyles: DesktopControlStyles;
  fields: string[];
  onChange: (values: CredentialValues) => void;
  onSubmit: () => void;
  values: CredentialValues;
}) {
  const needsUsername = fields.includes('username');
  const needsPassword = fields.includes('password') || fields.length === 0;
  const needsTarget = fields.includes('target');

  return (
    <View style={controlStyles.modalBackdrop}>
      <View style={controlStyles.credentialPanel}>
        <Text style={controlStyles.panelTitle}>Credentials</Text>
        {needsUsername && (
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={(username) => onChange({ ...values, username })}
            placeholder="Username"
            placeholderTextColor={chrome.muted}
            style={controlStyles.credentialInput}
            value={values.username}
          />
        )}
        {needsPassword && (
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={(password) => onChange({ ...values, password })}
            placeholder="Password"
            placeholderTextColor={chrome.muted}
            secureTextEntry
            style={controlStyles.credentialInput}
            value={values.password}
          />
        )}
        {needsTarget && (
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={(target) => onChange({ ...values, target })}
            placeholder="Target"
            placeholderTextColor={chrome.muted}
            style={controlStyles.credentialInput}
            value={values.target}
          />
        )}
        <ControlButton
          active
          controlStyles={controlStyles}
          icon={<Send color={chrome.onAccent} size={16} />}
          label="Connect"
          onPress={onSubmit}
        />
      </View>
    </View>
  );
}

function desktopWebSocketUrl(
  baseUrl: string,
  desktopHref: string,
  token: string,
): string {
  const cleanHref = desktopHref.replace(/\/+$/, '');
  const url = new URL(`${cleanHref}/ws`, `${normalizeBaseUrl(baseUrl)}/`);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  return url.toString();
}

function initialViewerState(
  viewOnly: boolean,
  screens: DesktopScreen[],
): DesktopViewerState {
  const normalizedScreens = normalizeScreens(screens);
  return {
    ready: false,
    connected: false,
    status: 'Connecting',
    statusIsError: false,
    viewOnly,
    autoScale: true,
    zoomLevel: 1,
    selectedScreenId: preferredScreenId(normalizedScreens),
    screens: normalizedScreens,
    pointerMode: 'touchpad',
    activeMouseButton: 0x1,
    dragLocked: false,
    pressedModifiers: [],
    credentialsRequired: null,
  };
}

function preferredScreenId(screens: DesktopViewerScreen[]): string {
  if (screens.length < 2) {
    return 'all';
  }

  return screens.find((screen) => screen.primary)?.id || screens[0]?.id || 'all';
}

function parseBridgeMessage(value: string): { type: string; state?: unknown } | null {
  try {
    const parsed = JSON.parse(value);
    if (isRecord(parsed) && typeof parsed.type === 'string') {
      return {
        type: parsed.type,
        state: parsed.state,
      };
    }
  } catch {}

  return null;
}

function mergeViewerState(
  current: DesktopViewerState,
  incoming: unknown,
): DesktopViewerState {
  if (!isRecord(incoming)) {
    return current;
  }

  return {
    ...current,
    ready: booleanValue(incoming.ready, current.ready),
    connected: booleanValue(incoming.connected, current.connected),
    status:
      typeof incoming.status === 'string' ? incoming.status : current.status,
    statusIsError: booleanValue(incoming.statusIsError, current.statusIsError),
    viewOnly: booleanValue(incoming.viewOnly, current.viewOnly),
    autoScale: booleanValue(incoming.autoScale, current.autoScale),
    zoomLevel: finiteNumber(incoming.zoomLevel, current.zoomLevel),
    selectedScreenId:
      typeof incoming.selectedScreenId === 'string'
        ? incoming.selectedScreenId
        : current.selectedScreenId,
    screens: Array.isArray(incoming.screens)
      ? normalizeScreens(incoming.screens)
      : current.screens,
    pointerMode: incoming.pointerMode === 'direct' ? 'direct' : 'touchpad',
    activeMouseButton: mouseButtonMask(incoming.activeMouseButton, current.activeMouseButton),
    dragLocked: booleanValue(incoming.dragLocked, current.dragLocked),
    pressedModifiers: Array.isArray(incoming.pressedModifiers)
      ? incoming.pressedModifiers.filter((modifier): modifier is string => typeof modifier === 'string')
      : current.pressedModifiers,
    credentialsRequired:
      incoming.credentialsRequired === null
        ? null
        : Array.isArray(incoming.credentialsRequired)
          ? incoming.credentialsRequired.filter((field): field is string => typeof field === 'string')
          : current.credentialsRequired,
  };
}

function normalizeScreens(value: unknown): DesktopViewerScreen[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((screen, index) => {
      if (!isRecord(screen)) {
        return null;
      }

      const width = finiteNumber(screen.width, 0);
      const height = finiteNumber(screen.height, 0);
      if (width <= 0 || height <= 0) {
        return null;
      }

      return {
        id: stringValue(screen.id, `screen-${index + 1}`),
        label: stringValue(screen.label, String(index + 1)),
        title: stringValue(screen.title, `Screen ${index + 1}`),
        x: finiteNumber(screen.x, 0),
        y: finiteNumber(screen.y, 0),
        width,
        height,
        primary: Boolean(screen.primary),
      };
    })
    .filter((screen): screen is DesktopViewerScreen => Boolean(screen));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}

function stringValue(value: unknown, fallback: string): string {
  return typeof value === 'string' && value ? value : fallback;
}

function booleanValue(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function finiteNumber(value: unknown, fallback: number): number {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

function mouseButtonMask(value: unknown, fallback: number): number {
  const mask = Number(value);
  return MOUSE_BUTTONS.some((button) => button.mask === mask) ? mask : fallback;
}

function createDesktopChrome(
  colors: ThemeColors,
  mode: ThemeMode,
): DesktopChrome {
  if (mode === 'dark') {
    return {
      surface: 'rgba(16, 21, 20, 0.94)',
      panel: 'rgba(16, 21, 20, 0.98)',
      input: '#050505',
      text: colors.text,
      muted: colors.muted,
      accent: colors.accent,
      onAccent: colors.onAccent,
      success: colors.success,
      danger: colors.danger,
      dangerBg: colors.dangerBg,
      border: colors.border,
      modalBackdrop: 'rgba(0, 0, 0, 0.42)',
      viewerBackground: '#050505',
    };
  }

  return {
    surface: 'rgba(255, 255, 255, 0.88)',
    panel: 'rgba(255, 255, 255, 0.96)',
    input: colors.background,
    text: colors.text,
    muted: colors.muted,
    accent: colors.accent,
    onAccent: colors.onAccent,
    success: colors.success,
    danger: colors.danger,
    dangerBg: colors.dangerBg,
    border: colors.border,
    modalBackdrop: 'rgba(15, 23, 42, 0.2)',
    viewerBackground: colors.panel,
  };
}

function buttonColor(chrome: DesktopChrome, active = false): string {
  return active ? chrome.onAccent : chrome.text;
}

type DesktopControlStyles = ReturnType<typeof createDesktopControlStyles>;

function createDesktopControlStyles(chrome: DesktopChrome) {
  return StyleSheet.create({
    overlay: {
      ...StyleSheet.absoluteFillObject,
      justifyContent: 'space-between',
      padding: 8,
    },
    railScroll: {
      flexGrow: 0,
      flexShrink: 0,
    },
    topBar: {
      minHeight: 42,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 8,
      paddingRight: 8,
    },
    bottomBar: {
      minHeight: 48,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 8,
      paddingRight: 8,
    },
    statusPill: {
      maxWidth: 150,
      minHeight: 38,
      flexShrink: 0,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 6,
      borderRadius: 8,
      paddingHorizontal: 10,
      backgroundColor: chrome.surface,
    },
    statusPillError: {
      backgroundColor: chrome.dangerBg,
    },
    statusText: {
      minWidth: 0,
      color: chrome.success,
      fontSize: 12,
      fontWeight: '900',
    },
    statusTextError: {
      color: chrome.danger,
    },
    screenList: {
      flexDirection: 'row',
      flexShrink: 0,
      alignItems: 'center',
      overflow: 'hidden',
      borderRadius: 8,
      backgroundColor: chrome.surface,
    },
    screenButtonText: {
      minWidth: 18,
      textAlign: 'center',
    },
    zoomGroup: {
      flexShrink: 0,
      flexDirection: 'row',
      alignItems: 'center',
      overflow: 'hidden',
      borderRadius: 8,
      backgroundColor: chrome.surface,
    },
    zoomText: {
      minWidth: 46,
      color: chrome.text,
      fontSize: 12,
      fontWeight: '900',
      textAlign: 'center',
    },
    segment: {
      flexShrink: 0,
      flexDirection: 'row',
      alignItems: 'center',
      overflow: 'hidden',
      borderRadius: 8,
      backgroundColor: chrome.surface,
    },
    controlButton: {
      minWidth: 38,
      minHeight: 38,
      flexShrink: 0,
      flexDirection: 'row',
      alignItems: 'center',
      justifyContent: 'center',
      gap: 5,
      borderRadius: 8,
      paddingHorizontal: 9,
      backgroundColor: chrome.surface,
    },
    controlButtonActive: {
      backgroundColor: chrome.accent,
    },
    controlButtonDisabled: {
      opacity: 0.48,
    },
    controlButtonPressed: {
      opacity: 0.76,
    },
    controlButtonText: {
      color: chrome.text,
      fontSize: 12,
      fontWeight: '900',
    },
    controlButtonTextActive: {
      color: chrome.onAccent,
    },
    controlButtonTextDisabled: {
      color: chrome.muted,
    },
    keyboardHost: {
      position: 'absolute',
      right: 8,
      bottom: 64,
      left: 8,
    },
    keyboardPanel: {
      maxHeight: '72%',
      gap: 8,
      borderRadius: 8,
      padding: 10,
      backgroundColor: chrome.panel,
      shadowColor: '#000',
      shadowOffset: { width: 0, height: 8 },
      shadowOpacity: 0.22,
      shadowRadius: 18,
      elevation: 8,
    },
    panelHeader: {
      minHeight: 38,
      flexDirection: 'row',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: 8,
    },
    panelTitle: {
      color: chrome.text,
      fontSize: 16,
      fontWeight: '900',
    },
    sendRow: {
      flexDirection: 'row',
      alignItems: 'stretch',
      gap: 8,
    },
    keyboardInput: {
      minHeight: 58,
      maxHeight: 120,
      flex: 1,
      minWidth: 0,
      borderWidth: 1,
      borderColor: chrome.border,
      borderRadius: 8,
      paddingHorizontal: 10,
      paddingVertical: 8,
      color: chrome.text,
      backgroundColor: chrome.input,
      fontSize: 14,
      fontWeight: '700',
    },
    keyboardTools: {
      flexGrow: 0,
    },
    keyboardToolsContent: {
      gap: 7,
      paddingBottom: 2,
    },
    keyRow: {
      flexDirection: 'row',
      flexWrap: 'wrap',
      gap: 7,
    },
    modalBackdrop: {
      ...StyleSheet.absoluteFillObject,
      alignItems: 'center',
      justifyContent: 'center',
      padding: 16,
      backgroundColor: chrome.modalBackdrop,
    },
    credentialPanel: {
      width: '100%',
      maxWidth: 380,
      gap: 10,
      borderRadius: 8,
      padding: 12,
      backgroundColor: chrome.panel,
    },
    credentialInput: {
      minHeight: 44,
      borderWidth: 1,
      borderColor: chrome.border,
      borderRadius: 8,
      paddingHorizontal: 12,
      color: chrome.text,
      backgroundColor: chrome.input,
      fontSize: 15,
      fontWeight: '700',
    },
  });
}
