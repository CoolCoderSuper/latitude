import { Link2, LockKeyhole, Share2, Trash2, X } from 'lucide-react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Modal,
  Pressable,
  ScrollView,
  Share,
  Text,
  TextInput,
  View,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { absoluteUrl, type LatitudePublicApi } from '../../api';
import {
  AppButton,
  EmptyState,
  IconButton,
  InlineNotice,
  LoadingBlock,
} from '../../components/ui';
import { useTheme } from '../../theme';
import type { DeploymentShare, DeploymentSummary } from '../../types';
import { errorMessage } from '../../utils/errors';
import { createShareManagerStyles } from './shareManagerStyles';

const EXPIRY_OPTIONS = [
  { label: 'Never', seconds: null },
  { label: '1 hour', seconds: 60 * 60 },
  { label: '1 day', seconds: 60 * 60 * 24 },
  { label: '7 days', seconds: 60 * 60 * 24 * 7 },
] as const;

export function ShareManagerModal({
  api,
  baseUrl,
  deployment,
  onClose,
  projectName,
}: {
  api: LatitudePublicApi;
  baseUrl: string;
  deployment: DeploymentSummary | null;
  onClose: () => void;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const shareStyles = useMemo(() => createShareManagerStyles(colors), [colors]);
  const [shares, setShares] = useState<DeploymentShare[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [deletingToken, setDeletingToken] = useState<string | null>(null);
  const [password, setPassword] = useState('');
  const [expirySeconds, setExpirySeconds] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const loadShares = useCallback(async () => {
    if (!deployment) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const allShares = await api.shares();
      setShares(
        allShares.filter(
          (share) =>
            share.project === projectName &&
            share.deployment === deployment.name,
        ),
      );
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [api, deployment, projectName]);

  useEffect(() => {
    if (deployment) {
      setPassword('');
      setExpirySeconds(null);
      setSuccess(null);
      void loadShares();
    }
  }, [deployment, loadShares]);

  const activeShares = useMemo(
    () =>
      [...shares].sort(
        (left, right) => Number(left.expired) - Number(right.expired),
      ),
    [shares],
  );

  const createShare = useCallback(async () => {
    if (!deployment || saving) {
      return;
    }
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      const trimmedPassword = password.trim();
      const share = await api.createShare({
        project: projectName,
        deployment: deployment.name,
        ...(trimmedPassword ? { password: trimmedPassword } : {}),
        ...(expirySeconds
          ? { expires_at: Math.floor(Date.now() / 1000) + expirySeconds }
          : {}),
      });
      setShares((current) => [...current, share]);
      setPassword('');
      setSuccess('Share link created.');
    } catch (createError) {
      setError(errorMessage(createError));
    } finally {
      setSaving(false);
    }
  }, [api, deployment, expirySeconds, password, projectName, saving]);

  const revokeShare = useCallback(
    (share: DeploymentShare) => {
      Alert.alert(
        'Revoke share link?',
        'Anyone using this link will lose access immediately.',
        [
          { text: 'Cancel', style: 'cancel' },
          {
            text: 'Revoke',
            style: 'destructive',
            onPress: () => {
              setDeletingToken(share.token);
              setError(null);
              void api
                .deleteShare(share.token)
                .then(() => {
                  setShares((current) =>
                    current.filter((item) => item.token !== share.token),
                  );
                  setSuccess('Share link revoked.');
                })
                .catch((deleteError) => setError(errorMessage(deleteError)))
                .finally(() => setDeletingToken(null));
            },
          },
        ],
      );
    },
    [api],
  );

  const sendShare = useCallback(
    async (share: DeploymentShare) => {
      const url = absoluteUrl(baseUrl, share.href);
      await Share.share({
        message: url,
        title: `Share ${projectName}/${share.deployment}`,
        url,
      });
    },
    [baseUrl, projectName],
  );

  return (
    <Modal
      animationType="slide"
      onRequestClose={onClose}
      presentationStyle="pageSheet"
      visible={deployment !== null}
    >
      <SafeAreaView style={shareStyles.safeArea}>
        <View style={shareStyles.header}>
          <View style={shareStyles.titleWrap}>
            <Text style={shareStyles.title}>
              Share {deployment?.name ?? ''}
            </Text>
            <Text style={shareStyles.subtitle}>{projectName}</Text>
          </View>
          <IconButton
            accessibilityLabel="Close share manager"
            icon={<X color={colors.text} size={21} />}
            onPress={onClose}
          />
        </View>
        <ScrollView
          contentContainerStyle={shareStyles.content}
          keyboardShouldPersistTaps="handled"
        >
          {error && <InlineNotice text={error} tone="error" />}
          {success && <InlineNotice text={success} tone="success" />}

          <View style={shareStyles.section}>
            <Text style={shareStyles.sectionTitle}>Create a link</Text>
            <Text style={shareStyles.helpText}>
              Leave the password blank for an open link. Share links bypass the
              server password.
            </Text>
            <View style={styles.formGroup}>
              <Text style={styles.label}>Password (optional)</Text>
              <TextInput
                autoCapitalize="none"
                autoCorrect={false}
                onChangeText={setPassword}
                placeholder="Add a link password"
                placeholderTextColor={colors.muted}
                secureTextEntry
                style={styles.input}
                value={password}
              />
            </View>
            <View style={styles.formGroup}>
              <Text style={styles.label}>Expires</Text>
              <View style={styles.quickRow}>
                {EXPIRY_OPTIONS.map((option) => {
                  const selected = expirySeconds === option.seconds;
                  return (
                    <Pressable
                      key={option.label}
                      onPress={() => setExpirySeconds(option.seconds)}
                      style={({ pressed }) => [
                        shareStyles.option,
                        selected && shareStyles.optionSelected,
                        pressed && styles.pressed,
                      ]}
                    >
                      <Text
                        style={[
                          shareStyles.optionText,
                          selected && shareStyles.optionTextSelected,
                        ]}
                      >
                        {option.label}
                      </Text>
                    </Pressable>
                  );
                })}
              </View>
            </View>
            <AppButton
              disabled={saving}
              icon={<Link2 color={colors.onAccent} size={18} />}
              label={saving ? 'Creating…' : 'Create share link'}
              onPress={() => void createShare()}
            />
          </View>

          <View style={shareStyles.section}>
            <View style={shareStyles.sectionHeadingRow}>
              <Text style={shareStyles.sectionTitle}>Existing links</Text>
              <AppButton
                compact
                disabled={loading}
                label="Refresh"
                onPress={() => void loadShares()}
                variant="secondary"
              />
            </View>
            {loading && shares.length === 0 ? (
              <LoadingBlock label="Loading share links" />
            ) : activeShares.length === 0 ? (
              <EmptyState title="No share links yet" />
            ) : (
              <View style={styles.list}>
                {activeShares.map((share) => (
                  <View key={share.token} style={shareStyles.card}>
                    <View style={shareStyles.cardHeading}>
                      <View style={styles.cardIcon}>
                        {share.has_password ? (
                          <LockKeyhole color={colors.accent} size={19} />
                        ) : (
                          <Link2 color={colors.accent} size={19} />
                        )}
                      </View>
                      <View style={styles.cardBody}>
                        <Text numberOfLines={1} style={styles.cardTitle}>
                          {share.token}
                        </Text>
                        <Text
                          style={[
                            styles.cardMeta,
                            share.expired && shareStyles.expiredText,
                          ]}
                        >
                          {share.expired
                            ? 'Expired'
                            : expiryLabel(share.expires_at)}
                          {share.has_password
                            ? ' · Password protected'
                            : ' · Open link'}
                        </Text>
                      </View>
                    </View>
                    <View style={shareStyles.actions}>
                      <AppButton
                        compact
                        disabled={share.expired}
                        icon={
                          <Share2
                            color={share.expired ? colors.muted : colors.text}
                            size={17}
                          />
                        }
                        label="Share"
                        onPress={() => void sendShare(share)}
                        variant="secondary"
                      />
                      <AppButton
                        compact
                        disabled={deletingToken === share.token}
                        icon={<Trash2 color={colors.danger} size={17} />}
                        label={
                          deletingToken === share.token ? 'Revoking…' : 'Revoke'
                        }
                        onPress={() => revokeShare(share)}
                        variant="danger"
                      />
                    </View>
                  </View>
                ))}
              </View>
            )}
          </View>
        </ScrollView>
      </SafeAreaView>
    </Modal>
  );
}

function expiryLabel(expiresAt: number | null): string {
  if (!expiresAt) {
    return 'Never expires';
  }
  return `Expires ${new Date(expiresAt * 1000).toLocaleString()}`;
}
