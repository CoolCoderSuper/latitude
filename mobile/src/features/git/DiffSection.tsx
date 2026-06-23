import { Download, Trash2, Upload } from 'lucide-react-native';
import { memo, useCallback, useMemo, useState } from 'react';
import {
  FlatList,
  Pressable,
  ScrollView,
  Text,
  View,
} from 'react-native';

import { useTheme } from '../../theme';
import type { DiffLine, GitFileChange, GitFileDiff } from '../../types';
import {
  statusLabel,
  visibleDiffsForSection,
} from './gitDiffUtils';
import {
  diffLineStyle,
  fallbackDiffLines,
  tokenStyle,
} from './diffSyntax';

const SELECTABLE_DIFF_LINE_LIMIT = 600;
const SELECTABLE_DIFF_TOKEN_LIMIT = 7000;

export function DiffSection({
  actioning,
  changes,
  empty,
  expanded,
  onAction,
  onCodeInteractionChange,
  onDiscard,
  onToggle,
  section,
  title,
}: {
  actioning: boolean;
  changes: GitFileChange[];
  empty: string;
  expanded: Set<string>;
  onAction: (change: GitFileChange) => void;
  onCodeInteractionChange: (active: boolean) => void;
  onDiscard?: (change: GitFileChange) => void;
  onToggle: (path: string) => void;
  section: 'unstaged' | 'staged';
  title: string;
}) {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.diffSection}>
      <View style={styles.sectionHeading}>
        <Text style={styles.sectionTitle}>{title}</Text>
        <Text style={styles.sectionCount}>
          {changes.length} {changes.length === 1 ? 'file' : 'files'}
        </Text>
      </View>
      {changes.length === 0 ? (
        <View style={styles.emptyPanel}>
          <Text style={styles.emptyText}>{empty}</Text>
        </View>
      ) : (
        changes.map((change) => {
          const isOpen = expanded.has(`${section}:${change.path}`);
          const visibleDiffs = visibleDiffsForSection(change, section);
          const actionLabel = section === 'unstaged' ? 'Stage' : 'Unstage';
          return (
            <View key={`${section}:${change.path}`} style={styles.fileCard}>
              <View style={styles.fileSummary}>
                <Pressable
                  onPress={() => onToggle(`${section}:${change.path}`)}
                  style={({ pressed }) => [
                    styles.fileSummaryMain,
                    pressed && styles.pressed,
                  ]}
                >
                  <Text style={styles.statusBadge}>{statusLabel(change)}</Text>
                  <View style={styles.cardBody}>
                    <Text numberOfLines={2} style={styles.filePath}>
                      {change.path}
                    </Text>
                    {change.original_path && (
                      <Text numberOfLines={1} style={styles.cardMeta}>
                        from {change.original_path}
                      </Text>
                    )}
                  </View>
                  <Text style={styles.cardMeta}>
                    {visibleDiffs.length === 0
                      ? 'status'
                      : `${visibleDiffs.length} diff${visibleDiffs.length === 1 ? '' : 's'}`}
                  </Text>
                </Pressable>
                <View style={styles.fileRowActions}>
                  <Pressable
                    accessibilityLabel={`${actionLabel} ${change.path}`}
                    disabled={actioning}
                    onPress={() => onAction(change)}
                    style={({ pressed }) => [
                      styles.fileRowAction,
                      actioning && styles.fileRowActionDisabled,
                      pressed && !actioning && styles.pressed,
                    ]}
                  >
                    {section === 'unstaged' ? (
                      <Upload color={colors.onAccent} size={14} />
                    ) : (
                      <Download color={colors.onAccent} size={14} />
                    )}
                    <Text style={styles.fileRowActionText}>{actionLabel}</Text>
                  </Pressable>
                  {section === 'unstaged' && onDiscard && (
                    <Pressable
                      accessibilityLabel={`Discard ${change.path}`}
                      disabled={actioning}
                      onPress={() => onDiscard(change)}
                      style={({ pressed }) => [
                        styles.fileRowAction,
                        styles.fileRowDangerAction,
                        actioning && styles.fileRowActionDisabled,
                        pressed && !actioning && styles.pressed,
                      ]}
                    >
                      <Trash2 color={colors.danger} size={14} />
                      <Text
                        style={[
                          styles.fileRowActionText,
                          styles.fileRowDangerActionText,
                        ]}
                      >
                        Discard
                      </Text>
                    </Pressable>
                  )}
                </View>
              </View>
              {isOpen && (
                <View style={styles.fileDetail}>
                  {visibleDiffs.length === 0 ? (
                    <Text style={styles.fileDetailEmptyText}>
                      No inline diff for this file.
                    </Text>
                  ) : (
                    visibleDiffs.map((fileDiff) => (
                      <DiffBlock
                        key={`${fileDiff.label}:${fileDiff.path}:${fileDiff.command}`}
                        diff={fileDiff}
                        onInteractionChange={onCodeInteractionChange}
                      />
                    ))
                  )}
                </View>
              )}
            </View>
          );
        })
      )}
    </View>
  );
}

type DiffRow = {
  line: DiffLine;
  text: string;
  key: string;
};

function DiffBlock({
  diff,
  onInteractionChange,
}: {
  diff: GitFileDiff;
  onInteractionChange: (active: boolean) => void;
}) {
  const { styles } = useTheme();
  const lines = diff.lines ?? fallbackDiffLines(diff.content);
  const [viewportWidth, setViewportWidth] = useState(1);
  const endInteraction = useCallback(() => {
    onInteractionChange(false);
  }, [onInteractionChange]);
  const rows = useMemo<DiffRow[]>(
    () =>
      lines.map((line, lineIndex) => {
        const text = line.tokens.map((token) => token.text).join('') || ' ';
        return {
          key: String(lineIndex),
          line,
          text,
        };
      }),
    [lines],
  );
  const tokenCount = useMemo(
    () => lines.reduce((count, line) => count + line.tokens.length, 0),
    [lines],
  );
  const useSelectableBlock =
    rows.length <= SELECTABLE_DIFF_LINE_LIMIT &&
    tokenCount <= SELECTABLE_DIFF_TOKEN_LIMIT;
  const estimatedContentWidth = useMemo(
    () =>
      Math.max(
        viewportWidth,
        rows.reduce(
          (width, row) =>
            Math.max(width, Math.min(row.text.length, 2200) * 7.2 + 24),
          0,
        ),
      ),
    [rows, viewportWidth],
  );
  const listHeight = Math.max(38, Math.min(520, rows.length * 19 + 16));
  const renderLine = useCallback(
    ({ item }: { item: DiffRow }) => <DiffLineRow row={item} />,
    [],
  );

  return (
    <View
      onLayout={(event) => {
        const nextWidth = Math.max(1, event.nativeEvent.layout.width);
        setViewportWidth((current) =>
          Math.abs(current - nextWidth) > 1 ? nextWidth : current,
        );
      }}
      style={styles.diffBlock}
    >
      <ScrollView
        horizontal
        contentContainerStyle={styles.diffScrollerContent}
        nestedScrollEnabled
        onMomentumScrollEnd={endInteraction}
        onScrollBeginDrag={() => onInteractionChange(true)}
        onScrollEndDrag={endInteraction}
        onTouchCancel={endInteraction}
        onTouchEnd={endInteraction}
        onTouchStart={() => onInteractionChange(true)}
        showsHorizontalScrollIndicator
        style={styles.diffScroller}
      >
        {useSelectableBlock ? (
          <SelectableDiffBlock rows={rows} />
        ) : (
          <FlatList
            data={rows}
            getItemLayout={(_data, index) => ({
              index,
              length: 19,
              offset: 19 * index,
            })}
            initialNumToRender={28}
            keyExtractor={(item) => item.key}
            maxToRenderPerBatch={36}
            nestedScrollEnabled
            onMomentumScrollEnd={endInteraction}
            onScrollBeginDrag={() => onInteractionChange(true)}
            onScrollEndDrag={endInteraction}
            removeClippedSubviews
            renderItem={renderLine}
            scrollEventThrottle={32}
            showsVerticalScrollIndicator={rows.length * 19 + 16 > listHeight}
            style={[
              styles.diffList,
              { height: listHeight, width: estimatedContentWidth },
            ]}
            updateCellsBatchingPeriod={16}
            windowSize={7}
          />
        )}
      </ScrollView>
    </View>
  );
}

function SelectableDiffBlock({ rows }: { rows: DiffRow[] }) {
  const { styles } = useTheme();

  return (
    <Text selectable style={styles.diffText}>
      {rows.map((row, lineIndex) => (
        <Text
          key={row.key}
          style={diffLineStyle(row.line.kind, styles)}
        >
          {row.line.tokens.map((token, tokenIndex) => (
            <Text
              key={`${lineIndex}:${tokenIndex}:${token.text}`}
              style={tokenStyle(token.kind, styles)}
            >
              {token.text}
            </Text>
          ))}
          {lineIndex < rows.length - 1 ? '\n' : ''}
        </Text>
      ))}
    </Text>
  );
}

const DiffLineRow = memo(function DiffLineRow({ row }: { row: DiffRow }) {
  const { styles } = useTheme();

  return (
    <Text
      numberOfLines={1}
      selectable
      style={[styles.diffLineText, diffLineStyle(row.line.kind, styles)]}
    >
      {row.line.tokens.map((token, tokenIndex) => (
        <Text
          key={`${tokenIndex}:${token.text}`}
          style={tokenStyle(token.kind, styles)}
        >
          {token.text}
        </Text>
      ))}
    </Text>
  );
});
