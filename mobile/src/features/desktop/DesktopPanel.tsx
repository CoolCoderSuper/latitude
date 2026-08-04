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

import { useTheme, type ThemeColors, type ThemeMode } from '../../theme';
import type { RootDesktopLink, SessionRecord } from '../../types';
import { commandInjectionScript } from '../../webview/bridge';
import { authenticatedWebSocketUrl } from '../../webview/urls';
import {
  DESKTOP_MOUSE_BUTTONS,
  initialDesktopViewerState,
  mergeDesktopViewerState,
  parseDesktopBridgeMessage,
  type DesktopCommand,
  type DesktopViewerState,
} from './desktopBridge';
import { desktopDocument } from './desktopDocument';

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
  const chrome = useMemo(
    () => createDesktopChrome(colors, mode),
    [colors, mode],
  );
  const controlStyles = useMemo(
    () => createDesktopControlStyles(chrome),
    [chrome],
  );
  const webViewRef = useRef<WebView>(null);
  const [viewerState, setViewerState] = useState<DesktopViewerState>(() =>
    initialDesktopViewerState(rootDesktop.view_only, rootDesktop.screens ?? []),
  );
  const [keyboardOpen, setKeyboardOpen] = useState(false);
  const [keyboardText, setKeyboardText] = useState('');
  const desktopUrl = useMemo(
    () =>
      authenticatedWebSocketUrl({
        baseUrl: session.baseUrl,
        href: rootDesktop.href,
        token: session.token,
      }),
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
  const canControl = !viewerState.viewOnly && viewerState.controlGranted;

  const sendCommand = useCallback((command: DesktopCommand) => {
    webViewRef.current?.injectJavaScript(
      commandInjectionScript('latitudeMobileCommand', command),
    );
  }, []);

  const requestState = useCallback(() => {
    sendCommand({ type: 'requestState' });
  }, [sendCommand]);

  useEffect(() => {
    setViewerState(
      initialDesktopViewerState(
        rootDesktop.view_only,
        rootDesktop.screens ?? [],
      ),
    );
    setKeyboardOpen(false);
    setKeyboardText('');
  }, [rootDesktop.screens, rootDesktop.view_only]);

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        sendCommand({ type: 'reconnect', force: true });
      } else {
        sendCommand({ type: 'releaseInput' });
      }
    });
    return () => subscription.remove();
  }, [sendCommand]);

  const handleMessage = useCallback(
    (event: { nativeEvent: { data: string } }) => {
      const message = parseDesktopBridgeMessage(event.nativeEvent.data);
      if (!message) {
        return;
      }

      setViewerState((current) =>
        mergeDesktopViewerState(current, message.state),
      );
    },
    [],
  );

  const sendKeyboardText = useCallback(() => {
    if (!keyboardText || !canControl) {
      return;
    }

    sendCommand({ type: 'sendText', text: keyboardText });
    setKeyboardText('');
  }, [canControl, keyboardText, sendCommand]);

  return (
    <View
      style={[
        controlStyles.panel,
        { backgroundColor: chrome.viewerBackground },
      ]}
    >
      <WebView
        ref={webViewRef}
        allowsInlineMediaPlayback
        bounces={false}
        domStorageEnabled
        javaScriptEnabled
        keyboardDisplayRequiresUserAction={false}
        mediaPlaybackRequiresUserAction={false}
        mixedContentMode="always"
        onLoadEnd={requestState}
        onMessage={handleMessage}
        originWhitelist={['*']}
        scrollEnabled={false}
        setSupportMultipleWindows={false}
        showsHorizontalScrollIndicator={false}
        showsVerticalScrollIndicator={false}
        source={{ html: desktopHtml, baseUrl: session.baseUrl }}
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
              color={viewerState.statusIsError ? chrome.danger : chrome.success}
              size={14}
            />
            <Text
              numberOfLines={1}
              style={[
                controlStyles.statusText,
                viewerState.statusIsError && controlStyles.statusTextError,
              ]}
            >
              {viewerState.status ||
                (viewerState.connected ? 'Connected' : 'Desktop')}
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
            icon={
              <Monitor
                color={buttonColor(chrome, viewerState.autoScale)}
                size={16}
              />
            }
            label={
              viewerState.autoScale && viewerState.zoomLevel <= 1.01
                ? 'Fit'
                : '1:1'
            }
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
                      sendCommand({
                        type: 'toggleModifier',
                        modifier: item.key,
                      }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={SPECIAL_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () =>
                      sendCommand({ type: 'pressKey', key: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={NAVIGATION_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () =>
                      sendCommand({ type: 'pressKey', key: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={ARROW_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () =>
                      sendCommand({ type: 'pressKey', key: item.key }),
                  }))}
                />
                <KeyRow
                  controlStyles={controlStyles}
                  items={SHORTCUT_KEYS.map((item) => ({
                    label: item.label,
                    onPress: () =>
                      sendCommand({
                        type: 'shortcut',
                        shortcut: item.shortcut,
                      }),
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
                      color={buttonColor(
                        chrome,
                        viewerState.pointerMode === 'touchpad',
                      )}
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
                      color={buttonColor(
                        chrome,
                        viewerState.pointerMode === 'direct',
                      )}
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
                {DESKTOP_MOUSE_BUTTONS.map((button) => (
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
                icon={
                  <Keyboard
                    color={buttonColor(chrome, keyboardOpen)}
                    size={17}
                  />
                }
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
  items: {
    active?: boolean;
    label: string;
    onPress: () => void;
  }[];
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
    viewerBackground: colors.panel,
  };
}

function buttonColor(chrome: DesktopChrome, active = false): string {
  return active ? chrome.onAccent : chrome.text;
}

type DesktopControlStyles = ReturnType<typeof createDesktopControlStyles>;

function createDesktopControlStyles(chrome: DesktopChrome) {
  return StyleSheet.create({
    panel: {
      flex: 1,
      backgroundColor: chrome.viewerBackground,
    },
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
  });
}
