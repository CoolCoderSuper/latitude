import { StyleSheet } from 'react-native';

import type { ThemeColors } from '../../theme';

export function createShareManagerStyles(colors: ThemeColors) {
  return StyleSheet.create({
    safeArea: { flex: 1, backgroundColor: colors.background },
    header: {
      minHeight: 68,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 12,
      borderBottomWidth: 1,
      borderBottomColor: colors.border,
      paddingHorizontal: 14,
      paddingVertical: 10,
      backgroundColor: colors.surface,
    },
    titleWrap: { flex: 1, minWidth: 0 },
    title: { color: colors.text, fontSize: 21, fontWeight: '900' },
    subtitle: {
      marginTop: 2,
      color: colors.muted,
      fontSize: 12,
      fontWeight: '700',
    },
    content: { gap: 14, padding: 14, paddingBottom: 36 },
    section: {
      gap: 14,
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      padding: 14,
      backgroundColor: colors.surface,
    },
    sectionHeadingRow: {
      flexDirection: 'row',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: 12,
    },
    sectionTitle: {
      flex: 1,
      color: colors.text,
      fontSize: 18,
      fontWeight: '900',
    },
    helpText: {
      color: colors.softText,
      fontSize: 14,
      lineHeight: 20,
      fontWeight: '600',
    },
    option: {
      minHeight: 38,
      justifyContent: 'center',
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      paddingHorizontal: 12,
      backgroundColor: colors.panel,
    },
    optionSelected: {
      borderColor: colors.accent,
      backgroundColor: colors.accent,
    },
    optionText: { color: colors.text, fontSize: 13, fontWeight: '800' },
    optionTextSelected: { color: colors.onAccent },
    card: {
      gap: 12,
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      padding: 12,
      backgroundColor: colors.panel,
    },
    cardHeading: { flexDirection: 'row', alignItems: 'center', gap: 12 },
    actions: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
    expiredText: { color: colors.danger },
  });
}
