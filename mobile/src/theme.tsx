import { createContext, useContext, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import {
  Platform,
  RefreshControl,
  StyleSheet,
  useColorScheme,
} from 'react-native';

import { loadThemeMode, saveThemeMode } from './storage';

export type ThemeMode = 'light' | 'dark';
export type ThemeColors = typeof lightColors;
export type AppStyles = ReturnType<typeof createStyles>;

type ThemeContextValue = {
  colors: ThemeColors;
  mode: ThemeMode;
  styles: AppStyles;
  toggleMode: () => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const systemScheme = useColorScheme();
  const [mode, setMode] = useState<ThemeMode>(
    systemScheme === 'dark' ? 'dark' : 'light',
  );
  const [loadedPreference, setLoadedPreference] = useState(false);
  const colors = mode === 'dark' ? darkColors : lightColors;
  const styles = useMemo(() => createStyles(colors), [colors]);

  useEffect(() => {
    let mounted = true;

    loadThemeMode()
      .then((storedMode) => {
        if (mounted && storedMode) {
          setMode(storedMode);
        }
      })
      .finally(() => {
        if (mounted) {
          setLoadedPreference(true);
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    if (loadedPreference) {
      void saveThemeMode(mode);
    }
  }, [loadedPreference, mode]);

  const value = useMemo(
    () => ({
      colors,
      mode,
      styles,
      toggleMode: () =>
        setMode((current) => (current === 'dark' ? 'light' : 'dark')),
    }),
    [colors, mode, styles],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme() {
  const value = useContext(ThemeContext);
  if (!value) {
    throw new Error('ThemeContext is missing.');
  }

  return value;
}

export function useRefreshControl(refreshing: boolean, onRefresh: () => void | Promise<void>) {
  const { colors } = useTheme();

  return (
    <RefreshControl
      colors={[colors.accent]}
      onRefresh={onRefresh}
      progressBackgroundColor={colors.surface}
      refreshing={refreshing}
      tintColor={colors.accent}
    />
  );
}

const lightColors = {
  background: '#f4f7f6',
  surface: '#ffffff',
  panel: '#eef3f1',
  text: '#18201f',
  softText: '#46524f',
  muted: '#75817e',
  border: '#d7dfdc',
  accent: '#0f766e',
  accentDark: '#115e59',
  onAccent: '#ffffff',
  danger: '#b42318',
  dangerBg: '#fff0ed',
  success: '#18794e',
  successBg: '#eaf7ef',
  gold: '#9a6b04',
  coral: '#c4553f',
  codeBg: '#f8fbfa',
  codeBorder: '#d9e4df',
  codeMuted: '#64736f',
  codeText: '#24302d',
  codeFile: '#25302d',
  codeHunkBg: '#e8f1f8',
  codeHunkText: '#245d9c',
  codeAddBg: '#eaf7ef',
  codeAddText: '#17643e',
  codeRemoveBg: '#fff0ed',
  codeRemoveText: '#a63a2c',
  tokenComment: '#71807c',
  tokenKeyword: '#7a4cc2',
  tokenNumber: '#08788a',
  tokenProperty: '#b03a5b',
  tokenPunctuation: '#73807d',
  tokenString: '#986900',
  tokenType: '#166aa3',
};

const darkColors = {
  background: '#101514',
  surface: '#171d1b',
  panel: '#202826',
  text: '#edf4f1',
  softText: '#c2cbc8',
  muted: '#8f9b97',
  border: '#2e3936',
  accent: '#2aa79c',
  accentDark: '#6dd4ca',
  onAccent: '#061413',
  danger: '#ffb3a7',
  dangerBg: '#351b18',
  success: '#8fe0ad',
  successBg: '#173323',
  gold: '#e1b95a',
  coral: '#ff9d87',
  codeBg: '#151b19',
  codeBorder: '#33413d',
  codeMuted: '#9da9a5',
  codeText: '#edf4f1',
  codeFile: '#e0e9e5',
  codeHunkBg: '#1d2d35',
  codeHunkText: '#9ec5ff',
  codeAddBg: '#183726',
  codeAddText: '#c8f2d5',
  codeRemoveBg: '#3c1e1a',
  codeRemoveText: '#ffd0ca',
  tokenComment: '#8c9995',
  tokenKeyword: '#c9b6ff',
  tokenNumber: '#73d7e7',
  tokenProperty: '#ffa5be',
  tokenPunctuation: '#9da9a5',
  tokenString: '#ffd98b',
  tokenType: '#9ed2ff',
};

function createStyles(colors: ThemeColors) {
  return StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: colors.background,
  },
  flex: {
    flex: 1,
  },
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 12,
  },
  loadingTitle: {
    color: colors.text,
    fontSize: 24,
    fontWeight: '800',
  },
  connectContent: {
    flexGrow: 1,
    justifyContent: 'center',
    gap: 18,
    padding: 20,
  },
  connectTopRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 12,
    marginBottom: 12,
  },
  brandRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    flexShrink: 1,
  },
  brandMark: {
    width: 52,
    height: 52,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.accent,
  },
  appName: {
    color: colors.text,
    fontSize: 34,
    fontWeight: '900',
  },
  appSubhead: {
    color: colors.softText,
    fontSize: 15,
    fontWeight: '700',
  },
  formGroup: {
    gap: 8,
  },
  label: {
    color: colors.softText,
    fontSize: 13,
    fontWeight: '800',
    textTransform: 'uppercase',
  },
  input: {
    minHeight: 48,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 13,
    color: colors.text,
    backgroundColor: colors.surface,
    fontSize: 16,
  },
  quickRow: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
  },
  chip: {
    minHeight: 34,
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 12,
    backgroundColor: colors.panel,
  },
  chipText: {
    color: colors.text,
    fontWeight: '700',
  },
  header: {
    paddingHorizontal: 14,
    paddingTop: 10,
    paddingBottom: 12,
    borderBottomWidth: 1,
    borderBottomColor: colors.border,
    backgroundColor: colors.surface,
  },
  headerTop: {
    minHeight: 48,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  headerSide: {
    minWidth: 44,
    alignItems: 'flex-end',
  },
  headerTextWrap: {
    flex: 1,
    minWidth: 0,
  },
  headerTitle: {
    color: colors.text,
    fontSize: 22,
    fontWeight: '900',
  },
  headerEyebrow: {
    marginTop: 2,
    color: colors.muted,
    fontSize: 12,
    fontWeight: '700',
  },
  headerActions: {
    flexDirection: 'row',
    gap: 8,
  },
  screenContent: {
    gap: 14,
    padding: 14,
    paddingBottom: 30,
  },
  serverSwitcher: {
    alignItems: 'center',
    gap: 8,
    paddingRight: 2,
  },
  serverPill: {
    maxWidth: 220,
    minHeight: 38,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 7,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 11,
    backgroundColor: colors.surface,
  },
  serverPillActive: {
    borderColor: colors.accent,
    backgroundColor: colors.accent,
  },
  serverPillText: {
    minWidth: 0,
    color: colors.text,
    fontSize: 13,
    fontWeight: '900',
  },
  serverPillTextActive: {
    color: colors.onAccent,
  },
  serverManagerList: {
    gap: 10,
  },
  serverManagerRow: {
    minHeight: 76,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 12,
    backgroundColor: colors.surface,
  },
  serverManagerRowActive: {
    borderColor: colors.accent,
  },
  serverManagerIcon: {
    width: 38,
    height: 38,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 8,
    backgroundColor: colors.panel,
  },
  serverManagerIconActive: {
    backgroundColor: colors.accent,
  },
  serverManagerBody: {
    flex: 1,
    minWidth: 0,
  },
  serverManagerTitle: {
    color: colors.text,
    fontSize: 16,
    fontWeight: '900',
  },
  serverManagerMeta: {
    marginTop: 3,
    color: colors.muted,
    fontSize: 12,
    fontWeight: '700',
  },
  serverManagerActions: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'flex-end',
    gap: 8,
  },
  serverActiveBadge: {
    minHeight: 40,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    borderRadius: 8,
    paddingHorizontal: 10,
    backgroundColor: colors.accent,
  },
  serverActiveText: {
    color: colors.onAccent,
    fontSize: 13,
    fontWeight: '900',
  },
  list: {
    gap: 10,
  },
  projectCard: {
    minHeight: 76,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 12,
    backgroundColor: colors.surface,
  },
  deploymentCard: {
    minHeight: 74,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 12,
    backgroundColor: colors.surface,
  },
  cardIcon: {
    width: 38,
    height: 38,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 8,
    backgroundColor: colors.panel,
  },
  cardBody: {
    flex: 1,
    minWidth: 0,
  },
  cardTitle: {
    color: colors.text,
    fontSize: 16,
    fontWeight: '900',
  },
  cardMeta: {
    marginTop: 3,
    color: colors.muted,
    fontSize: 13,
    fontWeight: '600',
  },
  segmented: {
    flexDirection: 'row',
    gap: 8,
    padding: 10,
    borderBottomWidth: 1,
    borderBottomColor: colors.border,
    backgroundColor: colors.surface,
  },
  segmentButton: {
    flex: 1,
    minWidth: 0,
    minHeight: 42,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.panel,
  },
  segmentButtonActive: {
    borderColor: colors.accent,
    backgroundColor: colors.accent,
  },
  segmentText: {
    color: colors.text,
    fontSize: 13,
    fontWeight: '900',
  },
  segmentTextActive: {
    color: colors.onAccent,
  },
  tabPager: {
    flex: 1,
    overflow: 'hidden',
    position: 'relative',
  },
  tabPage: {
    ...StyleSheet.absoluteFillObject,
    flex: 1,
  },
  tabPageActive: {
    opacity: 1,
    zIndex: 1,
  },
  tabPageHidden: {
    opacity: 0,
    zIndex: 0,
  },
  webView: {
    flex: 1,
    backgroundColor: colors.surface,
  },
  videoViewer: {
    flex: 1,
    backgroundColor: '#050505',
  },
  videoPlayer: {
    flex: 1,
    backgroundColor: '#050505',
  },
  imageViewer: {
    flex: 1,
    backgroundColor: '#050505',
  },
  nativeImage: {
    flex: 1,
    width: '100%',
    backgroundColor: '#050505',
  },
  mediaStatusOverlay: {
    ...StyleSheet.absoluteFillObject,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
    padding: 20,
    backgroundColor: 'rgba(5, 5, 5, 0.72)',
  },
  imageStatusText: {
    color: '#f8fafc',
    fontSize: 14,
    fontWeight: '800',
    textAlign: 'center',
  },
  imageErrorText: {
    color: '#fecaca',
    fontSize: 14,
    fontWeight: '800',
    textAlign: 'center',
  },
  terminalPanel: {
    flex: 1,
    backgroundColor: colors.background,
  },
  terminalSessionBar: {
    minHeight: 54,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    borderBottomWidth: 1,
    borderBottomColor: colors.border,
    paddingHorizontal: 10,
    paddingVertical: 8,
    backgroundColor: colors.surface,
  },
  terminalSessionList: {
    alignItems: 'center',
    gap: 8,
    paddingRight: 2,
  },
  terminalSessionItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  terminalSessionChip: {
    maxWidth: 156,
    minHeight: 36,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 10,
    backgroundColor: colors.panel,
  },
  terminalSessionChipActive: {
    borderColor: colors.accent,
    backgroundColor: colors.accent,
  },
  terminalSessionText: {
    minWidth: 0,
    color: colors.text,
    fontSize: 13,
    fontWeight: '900',
  },
  terminalSessionTextActive: {
    color: colors.onAccent,
  },
  terminalSessionClose: {
    width: 30,
    height: 30,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  terminalNewButton: {
    width: 38,
    height: 38,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 8,
    backgroundColor: colors.accent,
  },
  terminalStack: {
    flex: 1,
    position: 'relative',
    backgroundColor: colors.background,
  },
  terminalFrame: {
    ...StyleSheet.absoluteFillObject,
    opacity: 0,
    zIndex: 0,
  },
  terminalFrameActive: {
    opacity: 1,
    zIndex: 1,
  },
  button: {
    minHeight: 48,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    borderWidth: 1,
    borderColor: colors.accent,
    borderRadius: 8,
    paddingHorizontal: 16,
    backgroundColor: colors.accent,
  },
  buttonCompact: {
    minHeight: 40,
    alignSelf: 'flex-start',
    paddingHorizontal: 12,
  },
  buttonSecondary: {
    borderColor: colors.border,
    backgroundColor: colors.surface,
  },
  buttonDanger: {
    borderColor: colors.danger,
    backgroundColor: colors.dangerBg,
  },
  buttonDisabled: {
    borderColor: colors.border,
    backgroundColor: colors.panel,
  },
  buttonText: {
    color: colors.onAccent,
    fontSize: 15,
    fontWeight: '900',
  },
  buttonSecondaryText: {
    color: colors.text,
  },
  buttonDangerText: {
    color: colors.danger,
  },
  buttonDisabledText: {
    color: colors.muted,
  },
  iconButton: {
    width: 42,
    height: 42,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  pressed: {
    opacity: 0.72,
  },
  notice: {
    minHeight: 44,
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 8,
    borderWidth: 1,
    borderRadius: 8,
    padding: 10,
  },
  noticeError: {
    borderColor: '#f0b8ae',
    backgroundColor: colors.dangerBg,
  },
  noticeSuccess: {
    borderColor: '#b6dfc6',
    backgroundColor: colors.successBg,
  },
  noticeText: {
    flex: 1,
    fontSize: 14,
    fontWeight: '700',
  },
  noticeErrorText: {
    color: colors.danger,
  },
  noticeSuccessText: {
    color: colors.success,
  },
  loadingBlock: {
    minHeight: 96,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  emptyPanel: {
    minHeight: 86,
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 14,
    backgroundColor: colors.surface,
  },
  emptyTitle: {
    color: colors.text,
    fontSize: 16,
    fontWeight: '900',
  },
  emptyText: {
    color: colors.muted,
    fontSize: 14,
    fontWeight: '700',
  },
  diffToolbar: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: 8,
  },
  commitRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  commitInput: {
    flex: 1,
    minWidth: 0,
  },
  diffSection: {
    gap: 8,
  },
  sectionHeading: {
    minHeight: 38,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  sectionTitle: {
    color: colors.text,
    fontSize: 18,
    fontWeight: '900',
  },
  sectionCount: {
    color: colors.muted,
    fontSize: 12,
    fontWeight: '900',
    textTransform: 'uppercase',
  },
  fileCard: {
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  fileSummary: {
    minHeight: 64,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    padding: 10,
  },
  fileSummaryMain: {
    flex: 1,
    minWidth: 0,
    minHeight: 44,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  fileRowActions: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    justifyContent: 'flex-end',
    gap: 8,
  },
  fileRowAction: {
    minHeight: 36,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    borderRadius: 8,
    paddingHorizontal: 10,
    backgroundColor: colors.accent,
  },
  fileRowDangerAction: {
    backgroundColor: colors.dangerBg,
  },
  fileRowActionDisabled: {
    backgroundColor: colors.muted,
  },
  fileRowActionText: {
    color: colors.onAccent,
    fontSize: 12,
    fontWeight: '900',
  },
  fileRowDangerActionText: {
    color: colors.danger,
  },
  statusBadge: {
    minWidth: 38,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingVertical: 5,
    textAlign: 'center',
    color: colors.softText,
    fontFamily: Platform.select({
      ios: 'Menlo',
      android: 'monospace',
      default: 'monospace',
    }),
    fontWeight: '900',
    backgroundColor: colors.panel,
  },
  filePath: {
    color: colors.text,
    fontSize: 14,
    fontWeight: '900',
  },
  fileDetail: {
    borderTopWidth: 1,
    borderTopColor: colors.border,
    backgroundColor: colors.codeBg,
  },
  fileDetailEmptyText: {
    color: colors.muted,
    fontSize: 14,
    fontWeight: '700',
    padding: 12,
  },
  diffBlock: {
    overflow: 'hidden',
    backgroundColor: colors.codeBg,
  },
  diffScroller: {
    backgroundColor: colors.codeBg,
  },
  diffScrollerContent: {
    paddingBottom: 8,
    paddingRight: 16,
  },
  diffList: {
    backgroundColor: colors.codeBg,
  },
  diffText: {
    minWidth: '100%',
    paddingHorizontal: 10,
    paddingVertical: 8,
    color: colors.codeText,
    fontFamily: Platform.select({
      ios: 'Menlo',
      android: 'monospace',
      default: 'monospace',
    }),
    fontSize: 12,
    lineHeight: 19,
  },
  diffLineText: {
    height: 19,
    paddingHorizontal: 10,
    color: colors.codeText,
    fontFamily: Platform.select({
      ios: 'Menlo',
      android: 'monospace',
      default: 'monospace',
    }),
    fontSize: 12,
    lineHeight: 19,
  },
  diffLineFile: {
    color: colors.codeFile,
    fontWeight: '900',
  },
  diffLineHunk: {
    color: colors.codeHunkText,
    backgroundColor: colors.codeHunkBg,
  },
  diffLineAdd: {
    color: colors.codeAddText,
    backgroundColor: colors.codeAddBg,
  },
  diffLineRemove: {
    color: colors.codeRemoveText,
    backgroundColor: colors.codeRemoveBg,
  },
  tokenComment: {
    color: colors.tokenComment,
    fontStyle: 'italic',
  },
  tokenKeyword: {
    color: colors.tokenKeyword,
    fontWeight: '800',
  },
  tokenNumber: {
    color: colors.tokenNumber,
  },
  tokenProperty: {
    color: colors.tokenProperty,
  },
  tokenPunctuation: {
    color: colors.tokenPunctuation,
  },
  tokenString: {
    color: colors.tokenString,
  },
  tokenType: {
    color: colors.tokenType,
  },
  themeToggle: {
    marginLeft: 0,
  },
});
}
