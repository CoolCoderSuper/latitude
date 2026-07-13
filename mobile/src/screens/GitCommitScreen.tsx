import { ArrowLeft, ChevronDown, ChevronRight } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';

import type { LatitudePublicApi } from '../api';
import { EmptyState, IconButton, InlineNotice, LoadingBlock, ScreenHeader } from '../components/ui';
import { DiffBlock } from '../features/git/DiffSection';
import { useRefreshControl, useTheme } from '../theme';
import type { GitCommitResponse, GitFileDiff } from '../types';
import { errorMessage } from '../utils/errors';
import { appendDeviceHostname } from '../utils/headers';

export function GitCommitScreen({
  api,
  deviceHostname,
  hash,
  onBack,
  projectName,
}: {
  api: LatitudePublicApi;
  deviceHostname?: string;
  hash: string;
  onBack: () => void;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const [commit, setCommit] = useState<GitCommitResponse | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadCommit = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setCommit(await api.gitCommit(projectName, hash));
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [api, hash, projectName]);

  useEffect(() => {
    void loadCommit();
  }, [loadCommit]);

  const toggleFile = useCallback((path: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });
  }, []);

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={appendDeviceHostname(projectName, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back to Git history"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={commit?.subject ?? 'Commit'}
      />
      <ScrollView
        contentContainerStyle={styles.screenContent}
        nestedScrollEnabled
        refreshControl={useRefreshControl(loading, loadCommit)}
      >
        {error && <InlineNotice text={error} tone="error" />}
        {loading && !commit ? (
          <LoadingBlock label="Loading commit" />
        ) : commit ? (
          <>
            <View style={styles.gitCommitHeader}>
              <View style={styles.cardBody}>
                <Text selectable style={styles.gitHash}>{commit.short_hash}</Text>
                <Text numberOfLines={1} style={styles.gitHistoryMeta}>
                  {commit.author} · {formatCommitDate(commit.authored_at)}
                </Text>
              </View>
              <View style={styles.gitCommitStats}>
                <Text style={styles.gitCommitFileCount}>
                  {commit.files.length} {commit.files.length === 1 ? 'file' : 'files'}
                </Text>
                <Text style={styles.gitAdditionsText}>+{commit.additions}</Text>
                <Text style={styles.gitDeletionsText}>-{commit.deletions}</Text>
              </View>
            </View>
            {commit.files.length === 0 ? (
              <EmptyState title="No textual file diff" />
            ) : (
              <View style={styles.gitCommitFiles}>
                {commit.files.map((file) => (
                  <CommitFile
                    key={file.path}
                    expanded={expanded.has(file.path)}
                    file={file}
                    onToggle={() => toggleFile(file.path)}
                  />
                ))}
              </View>
            )}
          </>
        ) : (
          <EmptyState title="Commit unavailable" />
        )}
      </ScrollView>
    </View>
  );
}

function CommitFile({
  expanded,
  file,
  onToggle,
}: {
  expanded: boolean;
  file: GitFileDiff;
  onToggle: () => void;
}) {
  const { colors, styles } = useTheme();
  const counts = diffCounts(file.content);
  return (
    <View style={styles.fileCard}>
      <Pressable
        onPress={onToggle}
        style={({ pressed }) => [styles.gitCommitFileRow, pressed && styles.pressed]}
      >
        {expanded ? (
          <ChevronDown color={colors.muted} size={16} />
        ) : (
          <ChevronRight color={colors.muted} size={16} />
        )}
        <Text numberOfLines={1} style={styles.gitCommitFilePath}>{file.path}</Text>
        <View style={styles.gitCommitStats}>
          <Text style={styles.gitAdditionsText}>+{counts.additions}</Text>
          <Text style={styles.gitDeletionsText}>-{counts.deletions}</Text>
        </View>
      </Pressable>
      {expanded && (
        <View style={styles.fileDetail}>
          <DiffBlock diff={file} onInteractionChange={() => {}} />
        </View>
      )}
    </View>
  );
}

function diffCounts(content: string) {
  let additions = 0;
  let deletions = 0;
  content.split('\n').forEach((line) => {
    if (line.startsWith('+') && !line.startsWith('+++')) additions += 1;
    if (line.startsWith('-') && !line.startsWith('---')) deletions += 1;
  });
  return { additions, deletions };
}

function formatCommitDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
