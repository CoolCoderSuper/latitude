import {
  Download,
  GitCommitHorizontal,
  History,
  Rocket,
  Trash2,
  Upload,
} from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { Alert, ScrollView, Text, TextInput, View } from 'react-native';

import type { LatitudePublicApi } from '../../api';
import { AppButton, EmptyState, InlineNotice, LoadingBlock } from '../../components/ui';
import { useRefreshControl, useTheme } from '../../theme';
import type { GitActionPayload, GitDiffResponse, GitFileChange } from '../../types';
import { errorMessage } from '../../utils/errors';
import { DiffSection } from './DiffSection';
import { canStage, canUnstage, toggleExpanded } from './gitDiffUtils';

export function DiffPanel({
  api,
  onCodeInteractionChange,
  onOpenHistory,
  projectName,
}: {
  api: LatitudePublicApi;
  onCodeInteractionChange: (active: boolean) => void;
  onOpenHistory: () => void;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const [diff, setDiff] = useState<GitDiffResponse | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [actioning, setActioning] = useState(false);
  const [message, setMessage] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [noticeTone, setNoticeTone] = useState<'success' | 'error'>('success');

  const loadDiff = useCallback(async () => {
    setLoading(true);
    setNotice(null);
    try {
      setDiff(await api.diff(projectName));
    } catch (diffError) {
      setNotice(errorMessage(diffError));
      setNoticeTone('error');
    } finally {
      setLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadDiff();
  }, [loadDiff]);

  const runAction = useCallback(
    async (payload: GitActionPayload, successMessage: string) => {
      setActioning(true);
      setNotice(null);
      try {
        const response = await api.runGitAction(projectName, payload);
        setDiff(response.diff);
        setNotice(response.ok ? successMessage : response.error ?? 'Action failed.');
        setNoticeTone(response.ok ? 'success' : 'error');
        if (response.ok && payload.action === 'commit') {
          setMessage('');
        }
      } catch (actionError) {
        setNotice(errorMessage(actionError));
        setNoticeTone('error');
      } finally {
        setActioning(false);
      }
    },
    [api, projectName],
  );

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
            runAction(
              { action: 'discard_all' },
              'Unstaged changes discarded.',
            ),
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

  const unstaged = diff?.file_changes.filter(canStage) ?? [];
  const staged = diff?.file_changes.filter(canUnstage) ?? [];
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
          disabled={actioning || unstaged.length === 0}
          icon={<Upload color={colors.onAccent} size={16} />}
          label="Stage all"
          onPress={() =>
            runAction({ action: 'stage_all' }, 'All changes staged.')
          }
        />
        <AppButton
          compact
          disabled={actioning || staged.length === 0}
          icon={<Download color={colors.text} size={16} />}
          label="Unstage all"
          onPress={() =>
            runAction({ action: 'unstage_all' }, 'All changes unstaged.')
          }
          variant="secondary"
        />
        <AppButton
          compact
          disabled={actioning || unstaged.length === 0}
          icon={<Trash2 color={colors.danger} size={16} />}
          label="Discard all"
          onPress={confirmDiscardAll}
          variant="danger"
        />
      </View>

      <View style={styles.commitRow}>
        <TextInput
          editable={!actioning}
          onChangeText={setMessage}
          placeholder="Commit message"
          placeholderTextColor={colors.muted}
          style={[styles.input, styles.commitInput]}
          value={message}
        />
        <AppButton
          compact
          disabled={actioning || staged.length === 0 || !message.trim()}
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
          disabled={actioning}
          icon={<Download color={colors.text} size={16} />}
          label="Pull"
          onPress={() => runAction({ action: 'pull' }, 'Pull completed.')}
          variant="secondary"
        />
        <AppButton
          compact
          disabled={actioning}
          icon={<Rocket color={colors.text} size={16} />}
          label="Push"
          onPress={() => runAction({ action: 'push' }, 'Push completed.')}
          variant="secondary"
        />
      </View>

      {diff && (
        <View style={styles.gitOverview}>
          <Text style={styles.gitOverviewLabel}>Changes</Text>
          <Text style={styles.gitAdditionsText}>+{diff.additions}</Text>
          <Text style={styles.gitDeletionsText}>-{diff.deletions}</Text>
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
            actioning={actioning}
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
            onToggle={(path) => toggleExpanded(setExpanded, path)}
            section="unstaged"
            title="Unstaged"
          />
          <DiffSection
            actioning={actioning}
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
            onToggle={(path) => toggleExpanded(setExpanded, path)}
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
