import { ArrowLeft, ChevronRight } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, ScrollView, Text, View } from 'react-native';

import type { LatitudePublicApi } from '../api';
import { EmptyState, IconButton, InlineNotice, LoadingBlock, ScreenHeader } from '../components/ui';
import { useRefreshControl, useTheme } from '../theme';
import type { GitHistoryResponse } from '../types';
import { errorMessage } from '../utils/errors';
import { appendDeviceHostname } from '../utils/headers';

export function GitHistoryScreen({
  api,
  deviceHostname,
  onBack,
  onOpenCommit,
  projectName,
}: {
  api: LatitudePublicApi;
  deviceHostname?: string;
  onBack: () => void;
  onOpenCommit: (hash: string) => void;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const [history, setHistory] = useState<GitHistoryResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadHistory = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setHistory(await api.gitHistory(projectName));
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={appendDeviceHostname(projectName, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back to code changes"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title="Git history"
      />
      <ScrollView
        contentContainerStyle={styles.screenContent}
        refreshControl={useRefreshControl(loading, loadHistory)}
      >
        {error && <InlineNotice text={error} tone="error" />}
        {loading && !history ? (
          <LoadingBlock label="Loading Git history" />
        ) : history?.commits.length ? (
          <View style={styles.gitHistoryList}>
            {history.commits.map((commit) => (
              <Pressable
                key={commit.hash}
                accessibilityRole="button"
                onPress={() => onOpenCommit(commit.hash)}
                style={({ pressed }) => [styles.gitHistoryRow, pressed && styles.pressed]}
              >
                <Text style={styles.gitHash}>{commit.short_hash}</Text>
                <View style={styles.cardBody}>
                  <Text numberOfLines={1} style={styles.gitHistorySubject}>{commit.subject}</Text>
                  <Text numberOfLines={1} style={styles.gitHistoryMeta}>
                    {commit.author} · {formatCommitDate(commit.authored_at)}
                  </Text>
                </View>
                <ChevronRight color={colors.muted} size={18} />
              </Pressable>
            ))}
          </View>
        ) : (
          <EmptyState title="No commits found" />
        )}
      </ScrollView>
    </View>
  );
}

function formatCommitDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
