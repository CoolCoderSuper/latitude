import { Download, Upload } from 'lucide-react-native';
import { useCallback } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';

import { useTheme } from '../../theme';
import type { GitFileChange, GitFileDiff } from '../../types';
import {
  statusLabel,
  visibleDiffsForSection,
} from './gitDiffUtils';
import {
  diffLineStyle,
  renderDiffLineTokens,
  syntaxLanguageForPath,
  tokenStyle,
} from './diffSyntax';

export function DiffSection({
  actioning,
  changes,
  empty,
  expanded,
  onAction,
  onCodeInteractionChange,
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

function DiffBlock({
  diff,
  onInteractionChange,
}: {
  diff: GitFileDiff;
  onInteractionChange: (active: boolean) => void;
}) {
  const { styles } = useTheme();
  const lines = diff.content.length ? diff.content.split(/\r?\n/) : [];
  const language = syntaxLanguageForPath(diff.path);
  const endInteraction = useCallback(() => {
    onInteractionChange(false);
  }, [onInteractionChange]);

  return (
    <View style={styles.diffBlock}>
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
        <Text selectable style={styles.diffText}>
          {lines.map((line, lineIndex) => (
            <Text
              key={`${lineIndex}:${line}`}
              style={diffLineStyle(line, styles)}
            >
              {renderDiffLineTokens(line || ' ', language).map((token, tokenIndex) => (
                <Text
                  key={`${lineIndex}:${tokenIndex}:${token.text}`}
                  style={tokenStyle(token.kind, styles)}
                >
                  {token.text}
                </Text>
              ))}
              {lineIndex < lines.length - 1 ? '\n' : ''}
            </Text>
          ))}
        </Text>
      </ScrollView>
    </View>
  );
}
