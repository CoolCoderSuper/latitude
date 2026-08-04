import { StyleSheet } from 'react-native';

import type { ThemeColors } from '../../theme';

export function createTerminalStyles(colors: ThemeColors) {
  return StyleSheet.create({
    panel: { flex: 1, backgroundColor: colors.background },
    sessionBar: {
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
    sessionList: { alignItems: 'center', gap: 8, paddingRight: 2 },
    sessionItem: { flexDirection: 'row', alignItems: 'center', gap: 4 },
    sessionChip: {
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
    sessionChipActive: {
      borderColor: colors.accent,
      backgroundColor: colors.accent,
    },
    sessionText: {
      minWidth: 0,
      color: colors.text,
      fontSize: 13,
      fontWeight: '900',
    },
    sessionTextActive: { color: colors.onAccent },
    sessionClose: {
      width: 30,
      height: 30,
      alignItems: 'center',
      justifyContent: 'center',
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      backgroundColor: colors.surface,
    },
    newButton: {
      width: 38,
      height: 38,
      alignItems: 'center',
      justifyContent: 'center',
      borderRadius: 8,
      backgroundColor: colors.accent,
    },
    stack: {
      flex: 1,
      position: 'relative',
      backgroundColor: colors.background,
    },
    frame: { ...StyleSheet.absoluteFillObject, opacity: 0, zIndex: 0 },
    frameActive: { opacity: 1, zIndex: 1 },
  });
}
