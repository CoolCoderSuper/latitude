import {
  Download,
  GitCommitHorizontal,
  History,
  Rocket,
  Trash2,
  Upload,
} from 'lucide-react-native';
import { useCallback } from 'react';
import { Alert, ScrollView, Text, TextInput, View } from 'react-native';

import type { LatitudePublicApi } from '../../api';
import {
  AppButton,
  EmptyState,
  InlineNotice,
  LoadingBlock,
} from '../../components/ui';
import { useRefreshControl, useTheme } from '../../theme';
import type { GitFileChange } from '../../types';
import { DiffSection } from './DiffSection';
import { useGitDiffController } from './useGitDiffController';

export function DiffPanel({
  active,
  api,
  onCodeInteractionChange,
  onOpenHistory,
  projectName,
}: {
  active: boolean;
  api: LatitudePublicApi;
  onCodeInteractionChange: (active: boolean) => void;
  onOpenHistory: () => void;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const {
    diff,
    expanded,
    loadDiff,
    loading,
    message,
    notice,
    noticeTone,
    pendingActionKeys,
    runAction,
    selectedPaths,
    selectedStagedPaths,
    setMessage,
    staged,
    toggleFileExpanded,
    toggleSelected,
    toggleStagedSelected,
    unstaged,
  } = useGitDiffController({ active, api, projectName });

  const confirmDiscardAll = useCallback(() => {
    Alert.alert(
      'Discard changes?',
      'This will discard all unstaged changes and untracked files. It cannot be undone.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Discard',
          style: 'destructive',
          onPress: () =>
            runAction({ action: 'discard_all' }, 'Unstaged changes discarded.'),
        },
      ],
    );
  }, [runAction]);

  const confirmDiscardFile = useCallback(
    (change: GitFileChange) => {
      Alert.alert(
        'Discard file?',
        `Discard unstaged changes for ${change.path}? This cannot be undone.`,
        [
          { text: 'Cancel', style: 'cancel' },
          {
            text: 'Discard',
            style: 'destructive',
            onPress: () =>
              runAction(
                { action: 'discard_file', path: change.path },
                `${change.path} discarded.`,
              ),
          },
        ],
      );
    },
    [runAction],
  );

  const refreshControl = useRefreshControl(loading, loadDiff);

  return (
    <ScrollView
      contentContainerStyle={styles.screenContent}
      nestedScrollEnabled
      refreshControl={refreshControl}
    >
      <View style={styles.diffToolbar}>
        <AppButton
          compact
          disabled={
            pendingActionKeys.has('stage_all') ||
            pendingActionKeys.has('stage_selected') ||
            unstaged.length === 0
          }
          icon={<Upload color={colors.onAccent} size={16} />}
          label={
            selectedPaths.size > 0
              ? `Stage selected (${selectedPaths.size})`
              : 'Stage all'
          }
          onPress={() =>
            selectedPaths.size > 0
              ? runAction(
                  {
                    action: 'stage_selected',
                    paths: Array.from(selectedPaths),
                  },
                  `${selectedPaths.size} ${selectedPaths.size === 1 ? 'file' : 'files'} staged.`,
                )
              : runAction({ action: 'stage_all' }, 'All changes staged.')
          }
        />
        <AppButton
          compact
          disabled={
            pendingActionKeys.has('unstage_all') ||
            pendingActionKeys.has('unstage_selected') ||
            staged.length === 0
          }
          icon={<Download color={colors.text} size={16} />}
          label={
            selectedStagedPaths.size > 0
              ? `Unstage selected (${selectedStagedPaths.size})`
              : 'Unstage all'
          }
          onPress={() =>
            selectedStagedPaths.size > 0
              ? runAction(
                  {
                    action: 'unstage_selected',
                    paths: Array.from(selectedStagedPaths),
                  },
                  `${selectedStagedPaths.size} ${selectedStagedPaths.size === 1 ? 'file' : 'files'} unstaged.`,
                )
              : runAction({ action: 'unstage_all' }, 'All changes unstaged.')
          }
          variant="secondary"
        />
        <AppButton
          compact
          disabled={
            pendingActionKeys.has('discard_all') || unstaged.length === 0
          }
          icon={<Trash2 color={colors.danger} size={16} />}
          label="Discard all"
          onPress={confirmDiscardAll}
          variant="danger"
        />
      </View>

      <View style={styles.commitRow}>
        <TextInput
          editable={!pendingActionKeys.has('commit')}
          onChangeText={setMessage}
          placeholder="Commit message"
          placeholderTextColor={colors.muted}
          style={[styles.input, styles.commitInput]}
          value={message}
        />
        <AppButton
          compact
          disabled={
            pendingActionKeys.has('commit') ||
            staged.length === 0 ||
            !message.trim()
          }
          icon={<GitCommitHorizontal color={colors.onAccent} size={16} />}
          label="Commit"
          onPress={() =>
            runAction(
              { action: 'commit', message: message.trim() },
              'Staged changes committed.',
            )
          }
        />
      </View>

      <View style={styles.diffToolbar}>
        <AppButton
          compact
          icon={<History color={colors.text} size={16} />}
          label="History"
          onPress={onOpenHistory}
          variant="secondary"
        />
        <AppButton
          compact
          disabled={pendingActionKeys.has('pull')}
          icon={<Download color={colors.text} size={16} />}
          label="Pull"
          onPress={() => runAction({ action: 'pull' }, 'Pull completed.')}
          variant="secondary"
        />
        <AppButton
          compact
          disabled={pendingActionKeys.has('push')}
          icon={<Rocket color={colors.text} size={16} />}
          label="Push"
          onPress={() => runAction({ action: 'push' }, 'Push completed.')}
          variant="secondary"
        />
      </View>

      {diff && (
        <View style={styles.gitOverview}>
          <Text style={styles.gitOverviewLabel}>Changes</Text>
          {diff.additions > 0 && (
            <Text style={styles.gitAdditionsText}>+{diff.additions}</Text>
          )}
          {diff.deletions > 0 && (
            <Text style={styles.gitDeletionsText}>-{diff.deletions}</Text>
          )}
          <View style={styles.gitOverviewSpacer} />
          {diff.behind > 0 && (
            <Text style={styles.gitBehindText}>↓{diff.behind} pull</Text>
          )}
          {diff.ahead > 0 && (
            <Text style={styles.gitAheadText}>↑{diff.ahead} push</Text>
          )}
        </View>
      )}

      {notice && <InlineNotice tone={noticeTone} text={notice} />}

      {loading ? (
        <LoadingBlock label="Loading code changes" />
      ) : diff ? (
        <>
          <DiffSection
            changes={unstaged}
            empty="No unstaged files."
            expanded={expanded}
            onAction={(change) =>
              runAction(
                { action: 'stage_file', path: change.path },
                `${change.path} staged.`,
              )
            }
            onCodeInteractionChange={onCodeInteractionChange}
            onDiscard={confirmDiscardFile}
            onSelectionToggle={toggleSelected}
            onToggle={toggleFileExpanded}
            pendingActionKeys={pendingActionKeys}
            selectedPaths={selectedPaths}
            section="unstaged"
            title="Unstaged"
          />
          <DiffSection
            changes={staged}
            empty="No staged files."
            expanded={expanded}
            onAction={(change) =>
              runAction(
                { action: 'unstage_file', path: change.path },
                `${change.path} unstaged.`,
              )
            }
            onCodeInteractionChange={onCodeInteractionChange}
            onSelectionToggle={toggleStagedSelected}
            onToggle={toggleFileExpanded}
            pendingActionKeys={pendingActionKeys}
            selectedPaths={selectedStagedPaths}
            section="staged"
            title="Staged"
          />
        </>
      ) : (
        <EmptyState title="No diff available" />
      )}
    </ScrollView>
  );
}
