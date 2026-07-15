import {
  Download,
  GitCommitHorizontal,
  History,
  Rocket,
  Trash2,
  Upload,
} from 'lucide-react-native';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, AppState, ScrollView, Text, TextInput, View } from 'react-native';

import type { LatitudePublicApi } from '../../api';
import { AppButton, EmptyState, InlineNotice, LoadingBlock } from '../../components/ui';
import { useRefreshControl, useTheme } from '../../theme';
import type { GitActionPayload, GitDiffResponse, GitFileChange } from '../../types';
import { errorMessage } from '../../utils/errors';
import { DiffSection } from './DiffSection';
import { canStage, canUnstage, toggleExpanded } from './gitDiffUtils';

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
  const [diff, setDiff] = useState<GitDiffResponse | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [selectedStagedPaths, setSelectedStagedPaths] = useState<Set<string>>(
    new Set(),
  );
  const [loading, setLoading] = useState(true);
  const [pendingActionKeys, setPendingActionKeys] = useState<Set<string>>(
    new Set(),
  );
  const pendingActionKeysRef = useRef<Set<string>>(new Set());
  const actionQueue = useRef<Promise<void>>(Promise.resolve());
  const refreshPending = useRef(false);
  const [message, setMessage] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [noticeTone, setNoticeTone] = useState<'success' | 'error'>('success');

  const loadDiff = useCallback(async (showLoading = true) => {
    if (refreshPending.current) return;
    refreshPending.current = true;
    if (showLoading) {
      setLoading(true);
      setNotice(null);
    }
    try {
      setDiff(await api.diff(projectName));
    } catch (diffError) {
      if (showLoading) {
        setNotice(errorMessage(diffError));
        setNoticeTone('error');
      }
    } finally {
      refreshPending.current = false;
      if (showLoading) setLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadDiff();
  }, [loadDiff]);

  useEffect(() => {
    if (!active) return;

    let appActive = AppState.currentState === 'active';
    const refresh = () => {
      if (appActive) void loadDiff(false);
    };
    const fetchRemote = async () => {
      if (
        !appActive ||
        refreshPending.current ||
        pendingActionKeysRef.current.size > 0
      ) return;
      refreshPending.current = true;
      try {
        const response = await api.runGitAction(projectName, { action: 'fetch' });
        setDiff(response.diff);
      } catch {
        // Background fetch failures should not interrupt local diff refreshes.
      } finally {
        refreshPending.current = false;
      }
    };

    void fetchRemote();
    const refreshInterval = setInterval(refresh, 2_000);
    const fetchInterval = setInterval(() => void fetchRemote(), 30_000);
    const subscription = AppState.addEventListener('change', (state) => {
      const wasActive = appActive;
      appActive = state === 'active';
      if (appActive && !wasActive) {
        void fetchRemote();
        refresh();
      }
    });

    return () => {
      clearInterval(refreshInterval);
      clearInterval(fetchInterval);
      subscription.remove();
    };
  }, [active, api, loadDiff, projectName]);

  const runAction = useCallback(
    (payload: GitActionPayload, successMessage: string) => {
      const actionKey = gitActionKey(payload);
      if (pendingActionKeysRef.current.has(actionKey)) {
        return Promise.resolve();
      }

      pendingActionKeysRef.current.add(actionKey);
      setPendingActionKeys(new Set(pendingActionKeysRef.current));
      setNotice(null);

      const execute = async () => {
        try {
          const response = await api.runGitAction(projectName, payload);
          setDiff(response.diff);
          setNotice(
            response.ok ? successMessage : response.error ?? 'Action failed.',
          );
          setNoticeTone(response.ok ? 'success' : 'error');
          if (response.ok && payload.action === 'commit') {
            setMessage('');
          }
          if (response.ok && payload.action === 'stage_selected') {
            setSelectedPaths(new Set());
          }
          if (response.ok && payload.action === 'unstage_selected') {
            setSelectedStagedPaths(new Set());
          }
        } catch (actionError) {
          setNotice(errorMessage(actionError));
          setNoticeTone('error');
        } finally {
          pendingActionKeysRef.current.delete(actionKey);
          setPendingActionKeys(new Set(pendingActionKeysRef.current));
        }
      };

      const queuedAction = actionQueue.current.then(execute, execute);
      actionQueue.current = queuedAction;
      return queuedAction;
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

  useEffect(() => {
    const availablePaths = new Set(
      (diff?.file_changes ?? []).filter(canStage).map((change) => change.path),
    );
    setSelectedPaths((current) => {
      const next = new Set(
        Array.from(current).filter((path) => availablePaths.has(path)),
      );
      return next.size === current.size ? current : next;
    });
    const availableStagedPaths = new Set(
      (diff?.file_changes ?? []).filter(canUnstage).map((change) => change.path),
    );
    setSelectedStagedPaths((current) => {
      const next = new Set(
        Array.from(current).filter((path) => availableStagedPaths.has(path)),
      );
      return next.size === current.size ? current : next;
    });
  }, [diff]);

  const toggleSelected = useCallback((path: string) => {
    setSelectedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const toggleStagedSelected = useCallback((path: string) => {
    setSelectedStagedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

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
            onToggle={(path) => toggleExpanded(setExpanded, path)}
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
            onToggle={(path) => toggleExpanded(setExpanded, path)}
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

function gitActionKey(payload: GitActionPayload): string {
  return payload.path ? `${payload.action}:${payload.path}` : payload.action;
}
