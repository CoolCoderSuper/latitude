import { CheckCircle2, Server } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import {
  KeyboardAvoidingView,
  Platform,
  ScrollView,
  Text,
  TextInput,
  View,
} from 'react-native';

import { AppButton, Chip, InlineNotice } from '../components/ui';
import { ANDROID_EMULATOR_URL, DEFAULT_BASE_URL } from '../constants';
import { useTheme } from '../theme';
import { errorMessage } from '../utils/errors';

export function ConnectScreen({
  error,
  initialBaseUrl,
  onClearError,
  onLogin,
}: {
  error: string | null;
  initialBaseUrl: string;
  onClearError: () => void;
  onLogin: (baseUrl: string, password: string) => Promise<void>;
}) {
  const { colors, styles } = useTheme();
  const [baseUrl, setBaseUrl] = useState(initialBaseUrl);
  const [password, setPassword] = useState('');
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    setBaseUrl(initialBaseUrl);
  }, [initialBaseUrl]);

  const submit = useCallback(async () => {
    if (!baseUrl.trim() || !password) {
      return;
    }

    setSubmitting(true);
    onClearError();
    try {
      await onLogin(baseUrl, password);
    } catch (loginError) {
      onClearError();
      throw loginError;
    } finally {
      setSubmitting(false);
    }
  }, [baseUrl, onClearError, onLogin, password]);

  const [localError, setLocalError] = useState<string | null>(null);

  const submitWithError = useCallback(async () => {
    setLocalError(null);
    try {
      await submit();
    } catch (submitError) {
      setLocalError(errorMessage(submitError));
    }
  }, [submit]);

  return (
    <KeyboardAvoidingView
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      style={styles.flex}
    >
      <ScrollView
        contentContainerStyle={styles.connectContent}
        keyboardShouldPersistTaps="handled"
      >
        <View style={styles.brandRow}>
          <View style={styles.brandMark}>
            <Server color={colors.onAccent} size={28} />
          </View>
          <View>
            <Text style={styles.appName}>Latitude</Text>
            <Text style={styles.appSubhead}>Native client</Text>
          </View>
        </View>

        <View style={styles.formGroup}>
          <Text style={styles.label}>Public URL</Text>
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            onChangeText={setBaseUrl}
            placeholder={DEFAULT_BASE_URL}
            placeholderTextColor={colors.muted}
            style={styles.input}
            value={baseUrl}
          />
          <View style={styles.quickRow}>
            <Chip label="Localhost" onPress={() => setBaseUrl(DEFAULT_BASE_URL)} />
            <Chip
              label="Android"
              onPress={() => setBaseUrl(ANDROID_EMULATOR_URL)}
            />
          </View>
        </View>

        <View style={styles.formGroup}>
          <Text style={styles.label}>Password</Text>
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={setPassword}
            placeholder="Public password"
            placeholderTextColor={colors.muted}
            secureTextEntry
            style={styles.input}
            value={password}
          />
        </View>

        {(error || localError) && (
          <InlineNotice tone="error" text={localError ?? error ?? ''} />
        )}

        <AppButton
          disabled={submitting || !baseUrl.trim() || !password}
          icon={<CheckCircle2 color={colors.onAccent} size={18} />}
          label={submitting ? 'Signing in' : 'Sign in'}
          onPress={submitWithError}
        />
      </ScrollView>
    </KeyboardAvoidingView>
  );
}
