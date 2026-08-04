import { ArrowLeft, FileText, Search } from 'lucide-react-native';
import { useMemo, useState } from 'react';
import {
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import type { LatitudePublicApi } from '../../api';
import { EmptyState, IconButton, InlineNotice } from '../../components/ui';
import { useTheme } from '../../theme';
import type {
  ProjectFileSearchKind,
  ProjectFileSearchResult,
} from '../../types';
import { useFileSearch } from './useFileSearch';

export function FileSearchPanel({
  api,
  onClose,
  onOpenFile,
  projectName,
}: {
  api: LatitudePublicApi;
  onClose: () => void;
  onOpenFile: (result: ProjectFileSearchResult) => void;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const searchStyles = useMemo(() => createStyles(colors), [colors]);
  const [query, setQuery] = useState('');
  const [searchKind, setSearchKind] = useState<ProjectFileSearchKind>('file');
  const { error, limited, loading, results } = useFileSearch({
    api,
    projectName,
    query,
    searchKind,
  });
  const trimmedQuery = query.trim();

  return (
    <View style={searchStyles.container}>
      <View style={searchStyles.header}>
        <IconButton
          accessibilityLabel="Close file search"
          icon={<ArrowLeft color={colors.text} size={21} />}
          onPress={onClose}
        />
        <View style={searchStyles.headerText}>
          <Text style={searchStyles.title}>Search project files</Text>
          <Text style={searchStyles.subtitle}>
            {loading
              ? 'Searching…'
              : `${results.length}${limited ? '+' : ''} result${results.length === 1 ? '' : 's'}`}
          </Text>
        </View>
      </View>

      <View style={searchStyles.controls}>
        <View style={searchStyles.modeRow}>
          <SearchModeButton
            active={searchKind === 'file'}
            label="File names"
            onPress={() => setSearchKind('file')}
          />
          <SearchModeButton
            active={searchKind === 'grep'}
            label="File contents"
            onPress={() => setSearchKind('grep')}
          />
        </View>
        <View style={searchStyles.inputWrap}>
          <Search color={colors.muted} size={19} />
          <TextInput
            accessibilityLabel={
              searchKind === 'grep'
                ? 'Search file contents'
                : 'Search file names'
            }
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={setQuery}
            placeholder={
              searchKind === 'grep'
                ? 'Search file contents…'
                : 'Type a file name…'
            }
            placeholderTextColor={colors.muted}
            returnKeyType="search"
            style={searchStyles.input}
            value={query}
          />
        </View>
      </View>

      {error ? (
        <View style={searchStyles.notice}>
          <InlineNotice text={error} tone="error" />
        </View>
      ) : null}

      <FlatList
        contentContainerStyle={
          results.length === 0
            ? searchStyles.emptyContent
            : searchStyles.results
        }
        data={results}
        keyboardShouldPersistTaps="handled"
        keyExtractor={(item, index) =>
          `${item.path}:${item.line ?? 0}:${item.column ?? 0}:${index}`
        }
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            onPress={() => onOpenFile(item)}
            style={({ pressed }) => [
              searchStyles.result,
              pressed && styles.pressed,
            ]}
          >
            <FileText color={colors.accent} size={19} />
            <View style={searchStyles.resultBody}>
              <Text numberOfLines={1} style={searchStyles.resultPath}>
                {item.path}
                {item.line ? `:${item.line}:${item.column ?? 1}` : ''}
              </Text>
              {item.preview ? (
                <Text numberOfLines={2} style={searchStyles.preview}>
                  {item.preview}
                </Text>
              ) : null}
            </View>
          </Pressable>
        )}
        ListEmptyComponent={
          <EmptyState
            title={
              trimmedQuery
                ? loading
                  ? 'Searching…'
                  : 'No matches found'
                : searchKind === 'grep'
                  ? 'Type text to search file contents'
                  : 'Type part of a file name'
            }
          />
        }
      />
    </View>
  );
}

function SearchModeButton({
  active,
  label,
  onPress,
}: {
  active: boolean;
  label: string;
  onPress: () => void;
}) {
  const { colors } = useTheme();
  const searchStyles = useMemo(() => createStyles(colors), [colors]);

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected: active }}
      onPress={onPress}
      style={({ pressed }) => [
        searchStyles.modeButton,
        active && searchStyles.modeButtonActive,
        pressed && searchStyles.pressed,
      ]}
    >
      <Text
        style={[searchStyles.modeText, active && searchStyles.modeTextActive]}
      >
        {label}
      </Text>
    </Pressable>
  );
}

function createStyles(colors: ReturnType<typeof useTheme>['colors']) {
  return StyleSheet.create({
    container: { flex: 1, backgroundColor: colors.background },
    header: {
      minHeight: 64,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 10,
      borderBottomWidth: 1,
      borderBottomColor: colors.border,
      paddingHorizontal: 10,
      backgroundColor: colors.surface,
    },
    headerText: { flex: 1, minWidth: 0 },
    title: { color: colors.text, fontSize: 17, fontWeight: '900' },
    subtitle: {
      marginTop: 2,
      color: colors.muted,
      fontSize: 12,
      fontWeight: '700',
    },
    controls: {
      gap: 10,
      borderBottomWidth: 1,
      borderBottomColor: colors.border,
      padding: 10,
      backgroundColor: colors.panel,
    },
    modeRow: { flexDirection: 'row', gap: 8 },
    modeButton: {
      minHeight: 38,
      flex: 1,
      alignItems: 'center',
      justifyContent: 'center',
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      backgroundColor: colors.surface,
    },
    modeButtonActive: {
      borderColor: colors.accent,
      backgroundColor: colors.accent,
    },
    modeText: { color: colors.text, fontSize: 13, fontWeight: '900' },
    modeTextActive: { color: colors.onAccent },
    pressed: { opacity: 0.72 },
    inputWrap: {
      minHeight: 46,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 8,
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      paddingHorizontal: 12,
      backgroundColor: colors.surface,
    },
    input: { flex: 1, color: colors.text, fontSize: 16, paddingVertical: 10 },
    notice: { padding: 10, paddingBottom: 0 },
    results: { gap: 7, padding: 10, paddingBottom: 30 },
    emptyContent: { flexGrow: 1, justifyContent: 'center', padding: 14 },
    result: {
      minHeight: 58,
      flexDirection: 'row',
      alignItems: 'flex-start',
      gap: 10,
      borderWidth: 1,
      borderColor: colors.border,
      borderRadius: 8,
      padding: 11,
      backgroundColor: colors.surface,
    },
    resultBody: { flex: 1, minWidth: 0 },
    resultPath: { color: colors.text, fontSize: 14, fontWeight: '900' },
    preview: {
      marginTop: 5,
      color: colors.codeText,
      fontFamily: 'monospace',
      fontSize: 12,
      lineHeight: 17,
    },
  });
}
